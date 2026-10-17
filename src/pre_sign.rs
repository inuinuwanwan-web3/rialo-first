//! Read-only checks. No signing or sending; the public entry point is not wired to CLI.
pub mod lifecycle;
pub mod preparation;
use crate::{PreparedTransfer, Result, TESTNET_RPC, TRANSFER_AMOUNT};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rialo_cdk::{
    RpcClient,
    rpc::{HttpRpcClient, types::Pubkey},
};
use rialo_s_message::{ConfigHashPrefix, VersionedMessage};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Trusted injected clock: monotonic milliseconds must use one process-wide origin.
#[derive(Clone, Copy, Debug)]
pub struct ClockReading {
    pub unix_ms: i64,
    pub monotonic_ms: u64,
}
pub trait Clock {
    fn now(&self) -> Result<ClockReading>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Result<ClockReading> {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        let origin = ORIGIN.get_or_init(Instant::now);
        Ok(ClockReading {
            unix_ms: i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?,
            monotonic_ms: u64::try_from(origin.elapsed().as_millis())?,
        })
    }
}
pub const DEFAULT_TTL_MS: u64 = 30_000;
pub const MAX_TTL_MS: u64 = 300_000;
const CLOCK_DRIFT_TOLERANCE_MS: u64 = 1_000;

fn deadline(start: ClockReading, ttl: u64) -> Result<i64> {
    if start.unix_ms <= 0 || ttl == 0 || ttl > MAX_TTL_MS {
        return Err("不正な時刻または有効期間".into());
    }
    start
        .monotonic_ms
        .checked_add(ttl)
        .ok_or("時計オーバーフロー")?;
    start
        .unix_ms
        .checked_add(i64::try_from(ttl)?)
        .ok_or_else(|| "期限オーバーフロー".into())
}
fn validate_time(start: ClockReading, now: ClockReading, ttl: u64) -> Result<()> {
    let expires = deadline(start, ttl)?;
    let wall = now
        .unix_ms
        .checked_sub(start.unix_ms)
        .filter(|n| *n >= 0)
        .ok_or("時計の巻き戻り")? as u64;
    let elapsed = now
        .monotonic_ms
        .checked_sub(start.monotonic_ms)
        .ok_or("単調時計の巻き戻り")?;
    if wall.abs_diff(elapsed) > CLOCK_DRIFT_TOLERANCE_MS {
        return Err("時計の進み方が不一致".into());
    }
    if now.unix_ms >= expires || elapsed >= ttl {
        return Err("検証結果の期限切れ".into());
    }
    Ok(())
}

pub fn signing_message_hash(message: &[u8]) -> [u8; 32] {
    Sha256::digest(message).into()
}

/// Point-in-time observations, not a fee cap or independent chain identity proof.
#[derive(Debug, PartialEq)]
pub struct CheckReport {
    /// Unix epoch milliseconds; validity is [checked_at, expires_at).
    pub checked_at: i64,
    pub expires_at: i64,
    pub network_config_prefix: ConfigHashPrefix,
    pub network_check: bool,
    pub config_hash_prefix_match: bool,
    pub estimated_fee_kelvin: u64,
    pub estimated_fee_limit_kelvin: u64,
    pub balance_sufficient: bool,
    pub signing_message_hash: [u8; 32],
}

/// Only the successful check can create this value. No signing/assembly method is exposed.
pub struct CheckedTransfer {
    transfer: PreparedTransfer,
    report: CheckReport,
    checked_clock: ClockReading,
    ttl_ms: u64,
}
impl CheckedTransfer {
    /// Local validity only; fresh RPC revalidation is still required before requesting signing.
    pub fn validate_current(&self, clock: &impl Clock) -> Result<()> {
        self.validate_at(clock.now()?)
    }
    fn validate_at(&self, now: ClockReading) -> Result<()> {
        validate_time(self.checked_clock, now, self.ttl_ms)?;
        let r = &self.report;
        if r.checked_at != self.checked_clock.unix_ms
            || r.expires_at != deadline(self.checked_clock, self.ttl_ms)?
            || !r.network_check
            || !r.config_hash_prefix_match
            || !r.balance_sufficient
            || r.estimated_fee_kelvin > r.estimated_fee_limit_kelvin
            || TRANSFER_AMOUNT
                .checked_add(r.estimated_fee_kelvin)
                .is_none()
            || signing_message_hash(self.transfer.signing_message()) != r.signing_message_hash
        {
            return Err("不正または変更された検証結果".into());
        }
        let decoded: VersionedMessage = bincode::deserialize(self.transfer.signing_message())?;
        let VersionedMessage::Legacy(message) = decoded else {
            return Err("未対応メッセージ".into());
        };
        if message.config_hash_prefix != r.network_config_prefix
            || message.account_keys.first() != Some(&self.transfer.from)
        {
            return Err("公開鍵またはconfig prefix不一致".into());
        }
        Ok(())
    }
    pub fn report(&self) -> &CheckReport {
        &self.report
    }
    pub fn signing_message(&self) -> &[u8] {
        self.transfer.signing_message()
    }
}

/// Consumes a prepared message and compares it with a hash captured at review time.
/// All queries use one fixed Testnet client. This function performs RPC only when called.
pub async fn pre_sign_check(
    transfer: PreparedTransfer,
    reviewed_message_hash: [u8; 32],
    estimated_fee_limit_kelvin: u64,
) -> Result<CheckedTransfer> {
    pre_sign_check_with_clock(
        transfer,
        reviewed_message_hash,
        estimated_fee_limit_kelvin,
        DEFAULT_TTL_MS,
        &SystemClock,
    )
    .await
}

/// Configurable TTL; injected clock is a trusted dependency, not user-supplied request metadata.
pub async fn pre_sign_check_with_clock(
    transfer: PreparedTransfer,
    reviewed_hash: [u8; 32],
    limit: u64,
    ttl_ms: u64,
    clock: &impl Clock,
) -> Result<CheckedTransfer> {
    let rpc = TestnetReadRpc(HttpRpcClient::new(TESTNET_RPC.to_owned()));
    tokio::time::timeout(
        Duration::from_secs(30),
        check_with_rpc_and_clock(transfer, reviewed_hash, limit, &rpc, ttl_ms, clock),
    )
    .await
    .map_err(|_| "署名前チェックのRPCタイムアウト")?
}

/// Consumes old state even on failure; never renews expired results.
/// Uses the original message and original fee limit, with fresh Testnet queries.
pub async fn revalidate_before_signing(
    checked: CheckedTransfer,
    clock: &impl Clock,
) -> Result<CheckedTransfer> {
    let rpc = TestnetReadRpc(HttpRpcClient::new(TESTNET_RPC.to_owned()));
    tokio::time::timeout(
        Duration::from_secs(30),
        revalidate_with_rpc(checked, &rpc, clock),
    )
    .await
    .map_err(|_| "再検証のRPCタイムアウト")?
}
async fn revalidate_with_rpc(
    checked: CheckedTransfer,
    rpc: &impl ReadRpc,
    clock: &impl Clock,
) -> Result<CheckedTransfer> {
    checked.validate_current(clock)?;
    let old_start = checked.checked_clock;
    let ttl = checked.ttl_ms;
    let fresh = check_with_rpc_and_clock(
        checked.transfer,
        checked.report.signing_message_hash,
        checked.report.estimated_fee_limit_kelvin,
        rpc,
        ttl,
        clock,
    )
    .await?;
    // Old authorization must remain valid throughout revalidation; no expiry laundering.
    validate_time(old_start, fresh.checked_clock, ttl)?;
    let end = clock.now()?;
    validate_time(old_start, end, ttl)?;
    fresh.validate_at(end)?;
    Ok(fresh)
}

// Private adapter prevents callers from substituting an untrusted endpoint/provider.
trait ReadRpc {
    async fn prefix(&self) -> Result<ConfigHashPrefix>;
    async fn fee(&self, message_base64: &str) -> Result<Option<u64>>;
    async fn balance(&self, from: &Pubkey) -> Result<u64>;
}
struct TestnetReadRpc(HttpRpcClient);
impl ReadRpc for TestnetReadRpc {
    async fn prefix(&self) -> Result<ConfigHashPrefix> {
        Ok(self.0.get_config_hash_prefix().await?)
    }
    async fn fee(&self, message_base64: &str) -> Result<Option<u64>> {
        Ok(self.0.get_fee_for_message(message_base64).await?.value)
    }
    async fn balance(&self, from: &Pubkey) -> Result<u64> {
        Ok(self.0.get_balance(from).await?)
    }
}

#[cfg(test)]
async fn check_with_rpc(
    transfer: PreparedTransfer,
    reviewed_hash: [u8; 32],
    limit: u64,
    rpc: &impl ReadRpc,
) -> Result<CheckedTransfer> {
    check_with_rpc_and_clock(
        transfer,
        reviewed_hash,
        limit,
        rpc,
        DEFAULT_TTL_MS,
        &SystemClock,
    )
    .await
}

async fn check_with_rpc_and_clock(
    transfer: PreparedTransfer,
    reviewed_hash: [u8; 32],
    limit: u64,
    rpc: &impl ReadRpc,
    ttl_ms: u64,
    clock: &impl Clock,
) -> Result<CheckedTransfer> {
    let start = clock.now()?;
    let expires_at = deadline(start, ttl_ms)?;
    // Owned object and private fields keep the message immutable across all awaits.
    let bytes = transfer.signing_message();
    if signing_message_hash(bytes) != reviewed_hash {
        return Err("署名対象の改変".into());
    }
    let decoded: VersionedMessage = bincode::deserialize(bytes)?;
    if decoded.serialize_safe()? != bytes {
        return Err("メッセージ形式不一致".into());
    }
    let VersionedMessage::Legacy(message) = decoded else {
        return Err("未対応メッセージ".into());
    };
    if message.config_hash_prefix != rpc.prefix().await? {
        return Err("Testnet config hash prefix不一致".into());
    }
    // Encode exactly the checked VersionedMessage bytes, never reconstruct with new time/hash.
    let fee = rpc
        .fee(&STANDARD.encode(bytes))
        .await?
        .ok_or("手数料見積もり未取得")?;
    if fee > limit {
        return Err("手数料見積もり上限超過".into());
    }
    let total = transfer
        .amount_kelvin
        .checked_add(fee)
        .ok_or("必要残高オーバーフロー")?;
    let balance = rpc.balance(&transfer.from).await?;
    if balance < total {
        return Err("残高不足".into());
    }
    // Detect a configuration change during the non-atomic observation window.
    if message.config_hash_prefix != rpc.prefix().await? {
        return Err("照会中のconfig hash prefix変更".into());
    }
    if signing_message_hash(transfer.signing_message()) != reviewed_hash {
        return Err("照会中の署名対象改変".into());
    }
    validate_time(start, clock.now()?, ttl_ms)?;
    Ok(CheckedTransfer {
        checked_clock: start,
        ttl_ms,
        transfer,
        report: CheckReport {
            checked_at: start.unix_ms,
            expires_at,
            network_config_prefix: message.config_hash_prefix,
            network_check: true,
            config_hash_prefix_match: true,
            estimated_fee_kelvin: fee,
            estimated_fee_limit_kelvin: limit,
            balance_sufficient: true,
            signing_message_hash: reviewed_hash,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Mock {
        prefix: u64,
        fee: Option<u64>,
        balance: u64,
        fail: &'static str,
        expected: Vec<u8>,
        calls: Cell<usize>,
        change: bool,
    }
    impl ReadRpc for Mock {
        async fn prefix(&self) -> Result<ConfigHashPrefix> {
            if self.fail == "prefix" {
                return Err("mock RPC error".into());
            }
            let n = self.calls.get();
            self.calls.set(n + 1);
            Ok(ConfigHashPrefix::new(
                self.prefix + u64::from(self.change && n > 0),
            ))
        }
        async fn fee(&self, encoded: &str) -> Result<Option<u64>> {
            assert_eq!(STANDARD.decode(encoded)?, self.expected);
            if self.fail == "fee" {
                return Err("mock fee RPC error".into());
            }
            Ok(self.fee)
        }
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            if self.fail == "balance" {
                return Err("mock balance RPC error".into());
            }
            Ok(self.balance)
        }
    }
    fn fixture() -> (PreparedTransfer, [u8; 32], Mock) {
        // Public Ed25519 basepoint encoding, no private key generation or signatures.
        let mut key = [0x66; 32];
        key[0] = 0x58;
        let from = Pubkey::from(key).to_string();
        let to = Pubkey::from([2; 32]).to_string();
        let transfer =
            PreparedTransfer::new(TESTNET_RPC, &from, &to, 1234, ConfigHashPrefix::new(42))
                .unwrap();
        let expected = transfer.signing_message().to_vec();
        let hash = signing_message_hash(&expected);
        (
            transfer,
            hash,
            Mock {
                prefix: 42,
                fee: Some(5000),
                balance: TRANSFER_AMOUNT + 5000,
                fail: "",
                expected,
                calls: Cell::new(0),
                change: false,
            },
        )
    }
    #[tokio::test]
    async fn matching_prefix_and_exact_message_succeed() {
        let (tx, hash, rpc) = fixture();
        let checked = check_with_rpc(tx, hash, 5000, &rpc).await.unwrap();
        assert_eq!(
            checked.report(),
            &CheckReport {
                checked_at: checked.report().checked_at,
                expires_at: checked.report().expires_at,
                network_config_prefix: ConfigHashPrefix::new(42),
                network_check: true,
                config_hash_prefix_match: true,
                estimated_fee_kelvin: 5000,
                estimated_fee_limit_kelvin: 5000,
                balance_sufficient: true,
                signing_message_hash: hash
            }
        );
        assert_eq!(checked.signing_message(), rpc.expected);
        assert_eq!(rpc.calls.get(), 2);
    }
    #[tokio::test]
    async fn prefix_mismatch_stops() {
        let (tx, hash, mut rpc) = fixture();
        rpc.prefix = 43;
        assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
    }
    #[tokio::test]
    async fn rpc_errors_stop() {
        for stage in ["prefix", "fee", "balance"] {
            let (tx, hash, mut rpc) = fixture();
            rpc.fail = stage;
            assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
        }
    }
    #[tokio::test]
    async fn missing_fee_stops() {
        let (tx, hash, mut rpc) = fixture();
        rpc.fee = None;
        assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
    }
    #[tokio::test]
    async fn fee_over_limit_stops() {
        let (tx, hash, rpc) = fixture();
        assert!(check_with_rpc(tx, hash, 4999, &rpc).await.is_err());
    }
    #[tokio::test]
    async fn insufficient_balance_stops() {
        let (tx, hash, mut rpc) = fixture();
        rpc.balance -= 1;
        assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
    }
    #[tokio::test]
    async fn modified_message_stops_before_rpc() {
        let (mut tx, hash, rpc) = fixture();
        tx.transaction.message[0] ^= 1;
        assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
        assert_eq!(rpc.calls.get(), 0);
    }
    #[test]
    fn invalid_fee_data_is_rejected_by_cdk_response_type() {
        // Same type deserialized by HttpRpcClient::get_fee_for_message.
        use rialo_cdk::rpc::types::GetFeeForMessageResponse;
        for value in [
            "-1",
            "1.5",
            "\"5000\"",
            "18446744073709551616",
            "true",
            "{}",
        ] {
            let json = format!(r#"{{"version":0,"context":{{"slot":1}},"value":{value}}}"#);
            assert!(serde_json::from_str::<GetFeeForMessageResponse>(&json).is_err());
        }
        assert!(
            serde_json::from_str::<GetFeeForMessageResponse>(
                r#"{"version":0,"context":{"slot":1},"value":5000}"#
            )
            .is_ok()
        );
    }
    #[tokio::test]
    async fn overflowing_total_stops() {
        let (tx, hash, mut rpc) = fixture();
        rpc.fee = Some(u64::MAX);
        rpc.balance = u64::MAX;
        assert!(check_with_rpc(tx, hash, u64::MAX, &rpc).await.is_err());
    }
    #[tokio::test]
    async fn config_change_during_queries_stops() {
        let (tx, hash, mut rpc) = fixture();
        rpc.change = true;
        assert!(check_with_rpc(tx, hash, 5000, &rpc).await.is_err());
    }
    struct FakeClock {
        wall: Cell<i64>,
        mono: Cell<u64>,
    }
    impl FakeClock {
        fn new() -> Self {
            Self {
                wall: Cell::new(10_000),
                mono: Cell::new(100),
            }
        }
        fn advance(&self, ms: u64) {
            self.wall.set(self.wall.get() + ms as i64);
            self.mono.set(self.mono.get() + ms);
        }
    }
    impl Clock for FakeClock {
        fn now(&self) -> Result<ClockReading> {
            Ok(ClockReading {
                unix_ms: self.wall.get(),
                monotonic_ms: self.mono.get(),
            })
        }
    }
    async fn timed(clock: &impl Clock) -> (CheckedTransfer, Mock) {
        let (tx, hash, rpc) = fixture();
        let checked = check_with_rpc_and_clock(tx, hash, 6000, &rpc, 5000, clock)
            .await
            .unwrap();
        (checked, rpc)
    }
    #[tokio::test]
    async fn result_valid_before_deadline() {
        let clock = FakeClock::new();
        let (checked, _) = timed(&clock).await;
        assert_eq!(checked.report.checked_at, 10_000);
        assert_eq!(checked.report.expires_at, 15_000);
        clock.advance(4999);
        assert!(checked.validate_current(&clock).is_ok());
    }
    #[tokio::test]
    async fn result_expires_exactly_at_deadline() {
        let clock = FakeClock::new();
        let (checked, rpc) = timed(&clock).await;
        clock.advance(5000);
        let before = rpc.calls.get();
        assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
        assert_eq!(rpc.calls.get(), before);
    }
    #[tokio::test]
    async fn invalid_clock_and_ttl_stop_before_rpc() {
        for (wall, mono, ttl) in [
            (-1, 0, 100),
            (0, 0, 100),
            (i64::MAX, 0, 100),
            (10000, u64::MAX, 100),
            (10000, 0, 0),
            (10000, 0, MAX_TTL_MS + 1),
        ] {
            let clock = FakeClock {
                wall: Cell::new(wall),
                mono: Cell::new(mono),
            };
            let (tx, hash, rpc) = fixture();
            assert!(
                check_with_rpc_and_clock(tx, hash, 6000, &rpc, ttl, &clock)
                    .await
                    .is_err()
            );
            assert_eq!(rpc.calls.get(), 0);
        }
    }
    #[tokio::test]
    async fn backward_and_divergent_clocks_stop() {
        for (wall, mono) in [(9999, 100), (10000, 99), (12000, 100), (10000, 2100)] {
            let clock = FakeClock::new();
            let (checked, _) = timed(&clock).await;
            clock.wall.set(wall);
            clock.mono.set(mono);
            assert!(checked.validate_current(&clock).is_err());
        }
    }
    #[tokio::test]
    async fn clock_read_failure_stops() {
        struct Broken;
        impl Clock for Broken {
            fn now(&self) -> Result<ClockReading> {
                Err("clock unavailable".into())
            }
        }
        let (tx, hash, rpc) = fixture();
        assert!(
            check_with_rpc_and_clock(tx, hash, 6000, &rpc, 5000, &Broken)
                .await
                .is_err()
        );
        assert_eq!(rpc.calls.get(), 0);
    }
    async fn rejects_changed_field(field: &str) {
        let clock = FakeClock::new();
        let (mut checked, rpc) = timed(&clock).await;
        let decoded: VersionedMessage =
            bincode::deserialize(checked.transfer.signing_message()).unwrap();
        let VersionedMessage::Legacy(mut m) = decoded else {
            panic!("legacy expected");
        };
        match field {
            "recipient" => {
                let i = m.instructions[0].accounts[1] as usize;
                m.account_keys[i] = Pubkey::from([3; 32]);
            }
            "amount" => m.instructions[0].data[4..12].copy_from_slice(&2_000_000u64.to_le_bytes()),
            "public_key" => m.account_keys[0] = Pubkey::from([4; 32]),
            "message" => m.valid_from += 1,
            _ => panic!("unknown field"),
        }
        checked.transfer.transaction.message =
            VersionedMessage::Legacy(m).serialize_safe().unwrap();
        let before = rpc.calls.get();
        assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
        assert_eq!(rpc.calls.get(), before);
    }
    #[tokio::test]
    async fn revalidation_rejects_message_change() {
        rejects_changed_field("message").await;
    }
    #[tokio::test]
    async fn revalidation_rejects_recipient_change() {
        rejects_changed_field("recipient").await;
    }
    #[tokio::test]
    async fn revalidation_rejects_amount_change() {
        rejects_changed_field("amount").await;
    }
    #[tokio::test]
    async fn revalidation_rejects_public_key_change() {
        rejects_changed_field("public_key").await;
    }
    #[tokio::test]
    async fn revalidation_rejects_network_change() {
        let clock = FakeClock::new();
        let (checked, mut rpc) = timed(&clock).await;
        rpc.prefix = 99;
        assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
    }
    #[tokio::test]
    async fn revalidation_refreshes_increased_fee_within_original_limit() {
        let clock = FakeClock::new();
        let (checked, mut rpc) = timed(&clock).await;
        clock.advance(1000);
        rpc.fee = Some(5500);
        rpc.balance = TRANSFER_AMOUNT + 5500;
        let fresh = revalidate_with_rpc(checked, &rpc, &clock).await.unwrap();
        assert_eq!(fresh.report.estimated_fee_kelvin, 5500);
        assert_eq!(fresh.report.estimated_fee_limit_kelvin, 6000);
        assert_eq!(fresh.report.checked_at, 11000);
        assert_eq!(fresh.report.expires_at, 16000);
        assert_eq!(rpc.calls.get(), 4);
    }
    #[tokio::test]
    async fn revalidation_rejects_fee_limit_excess() {
        let clock = FakeClock::new();
        let (checked, mut rpc) = timed(&clock).await;
        rpc.fee = Some(6001);
        assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
    }
    #[tokio::test]
    async fn revalidation_rejects_insufficient_balance() {
        let clock = FakeClock::new();
        let (checked, mut rpc) = timed(&clock).await;
        rpc.balance = TRANSFER_AMOUNT;
        assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
    }
    #[tokio::test]
    async fn revalidation_rejects_rpc_errors_and_missing_fee() {
        for failure in ["prefix", "fee", "balance", "missing"] {
            let clock = FakeClock::new();
            let (checked, mut rpc) = timed(&clock).await;
            rpc.fail = failure;
            if failure == "missing" {
                rpc.fee = None;
            }
            assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
        }
    }
    #[tokio::test]
    async fn revalidation_rejects_corrupt_or_missing_report_data() {
        for field in 0..6 {
            let clock = FakeClock::new();
            let (mut checked, rpc) = timed(&clock).await;
            match field {
                0 => checked.report.network_check = false,
                1 => checked.report.expires_at = 0,
                2 => checked.report.network_config_prefix = ConfigHashPrefix::new(99),
                3 => checked.report.signing_message_hash = [0; 32],
                4 => checked.report.estimated_fee_limit_kelvin = 0,
                _ => checked.transfer.from = Pubkey::from([4; 32]),
            }
            let before = rpc.calls.get();
            assert!(revalidate_with_rpc(checked, &rpc, &clock).await.is_err());
            assert_eq!(rpc.calls.get(), before);
        }
    }
    struct SequenceClock(std::cell::RefCell<std::collections::VecDeque<ClockReading>>);
    impl Clock for SequenceClock {
        fn now(&self) -> Result<ClockReading> {
            self.0
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| "clock exhausted".into())
        }
    }
    fn sequence(times: &[i64]) -> SequenceClock {
        SequenceClock(std::cell::RefCell::new(
            times
                .iter()
                .map(|t| ClockReading {
                    unix_ms: *t,
                    monotonic_ms: (*t - 9900) as u64,
                })
                .collect(),
        ))
    }
    #[tokio::test]
    async fn initial_check_expiring_during_rpc_is_rejected() {
        let (tx, hash, rpc) = fixture();
        assert!(
            check_with_rpc_and_clock(tx, hash, 6000, &rpc, 5000, &sequence(&[10000, 15000]))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn revalidation_cannot_renew_old_result_that_expires_during_rpc() {
        let clock = FakeClock::new();
        let (checked, rpc) = timed(&clock).await;
        assert!(
            revalidate_with_rpc(checked, &rpc, &sequence(&[14999, 14999, 15000, 15000]))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn revalidation_success_with_unchanged_state() {
        let clock = FakeClock::new();
        let (checked, rpc) = timed(&clock).await;
        let expected_hash = checked.report.signing_message_hash;
        clock.advance(100);
        let fresh = revalidate_with_rpc(checked, &rpc, &clock).await.unwrap();
        assert_eq!(fresh.report.signing_message_hash, expected_hash);
        assert!(fresh.validate_current(&clock).is_ok());
        assert_eq!(rpc.calls.get(), 4);
    }
}
