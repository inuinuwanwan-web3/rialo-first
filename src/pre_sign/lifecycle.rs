//! In-memory lifecycle only: no signer transport, wallet access, or TX submission.
#[cfg(unix)]
pub mod persistent;
use super::*;
use crate::ExternalSignature;
use std::sync::atomic::{AtomicU64, Ordering};

pub const PROTOCOL_VERSION: u16 = 1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Created,
    Validated,
    AwaitingSignature,
    Signed,
    Rejected,
    Expired,
}

/// Raw message and raw SHA-256 bytes. Fee fields are estimates, never on-chain caps.
#[derive(Debug, Clone, PartialEq)]
pub struct SignatureRequest {
    pub protocol_version: u16,
    pub request_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub network: String,
    pub rpc_endpoint: String,
    pub network_config_prefix: ConfigHashPrefix,
    pub sender_address: String,
    pub recipient_address: String,
    pub amount_kelvin: u64,
    pub estimated_fee_kelvin: u64,
    pub estimated_fee_limit_kelvin: u64,
    pub signing_message: Vec<u8>,
    pub signing_message_hash: [u8; 32],
    pub expected_public_key: String,
}
#[derive(Debug, Clone)]
pub struct SignatureResponse {
    pub protocol_version: u16,
    pub request_id: String,
    pub public_key: String,
    pub signature: Vec<u8>,
    pub signing_message_hash: [u8; 32],
}

/// Session is not Clone/deserializable. Terminal states never regain checked state.
pub struct SignatureLifecycle {
    state: State,
    checked: Option<CheckedTransfer>,
    request: Option<SignatureRequest>,
    id: String,
}
impl SignatureLifecycle {
    pub fn new(checked: CheckedTransfer) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "要求ID枯渇")?;
        Ok(Self {
            state: State::Created,
            checked: Some(checked),
            request: None,
            id: format!("rialo-v1-{id}"),
        })
    }
    pub fn state(&self) -> State {
        self.state
    }

    /// Explicit opt-in read-only RPC revalidation; no request is exposed before success.
    pub async fn validate(&mut self, clock: &impl Clock) -> Result<()> {
        self.validate_using(clock, |checked| revalidate_before_signing(checked, clock))
            .await
    }
    async fn validate_using<F: std::future::Future<Output = Result<CheckedTransfer>>>(
        &mut self,
        clock: &impl Clock,
        revalidate: impl FnOnce(CheckedTransfer) -> F,
    ) -> Result<()> {
        let checked = self.take_for_validation(clock)?;
        // Rejected is set before await: cancellation cannot leave a reusable session.
        let fresh = revalidate(checked).await?;
        self.install(fresh, clock)
    }
    fn take_for_validation(&mut self, clock: &impl Clock) -> Result<CheckedTransfer> {
        if self.state != State::Created {
            return self.fail("不正な状態遷移");
        }
        self.check_time(clock)?;
        self.state = State::Rejected;
        self.checked.take().ok_or_else(|| "検証結果欠損".into())
    }
    fn install(&mut self, checked: CheckedTransfer, clock: &impl Clock) -> Result<()> {
        self.checked = Some(checked);
        let now = self.check_time(clock)?;
        let request = match request_from_checked(
            self.checked.as_ref().ok_or("検証結果欠損")?,
            &self.id,
            now.unix_ms,
        ) {
            Ok(r) => r,
            Err(e) => {
                self.reject();
                return Err(e);
            }
        };
        self.request = Some(request);
        self.state = State::Validated;
        Ok(())
    }
    /// Returns a copy for an external transport to display/sign. No transport is invoked.
    pub fn begin_signing(&mut self, clock: &impl Clock) -> Result<SignatureRequest> {
        if self.state != State::Validated {
            return self.fail("署名要求を開始できない状態");
        }
        self.check_time(clock)?;
        self.check_request()?;
        self.state = State::AwaitingSignature;
        Ok(self.request.as_ref().ok_or("署名要求欠損")?.clone())
    }
    /// One response attempt. Any invalid response permanently terminates this session.
    pub fn accept_response(
        &mut self,
        response: SignatureResponse,
        clock: &impl Clock,
    ) -> Result<Vec<u8>> {
        if self.state != State::AwaitingSignature {
            return self.fail("応答を受理できない状態");
        }
        self.check_time(clock)?;
        self.check_request()?;
        let r = self.request.as_ref().ok_or("署名要求欠損")?;
        if response.protocol_version != PROTOCOL_VERSION
            || response.protocol_version != r.protocol_version
            || response.request_id != r.request_id
            || response.public_key != r.expected_public_key
            || response.signing_message_hash != r.signing_message_hash
        {
            return self.fail("署名応答の識別情報不一致");
        }
        let checked = self.checked.take().ok_or("検証結果欠損")?;
        self.state = State::Rejected;
        let wire = checked.transfer.assemble(&[ExternalSignature {
            public_address: &response.public_key,
            signature: &response.signature,
        }])?;
        // No wire bytes escape if expiry occurs during cryptographic validation/assembly.
        let now = clock.now()?;
        if now.unix_ms >= checked.report.expires_at
            || now
                .monotonic_ms
                .checked_sub(checked.checked_clock.monotonic_ms)
                .is_some_and(|n| n >= checked.ttl_ms)
        {
            self.state = State::Expired;
        }
        validate_time(checked.checked_clock, now, checked.ttl_ms)?;
        if now.unix_ms < r.created_at {
            return Err("要求作成前への時計巻き戻り".into());
        }
        self.state = State::Signed;
        Ok(wire)
    }
    fn check_request(&mut self) -> Result<()> {
        let result = (|| {
            let r = self.request.as_ref().ok_or("署名要求欠損")?;
            let expected = request_from_checked(
                self.checked.as_ref().ok_or("検証結果欠損")?,
                &self.id,
                r.created_at,
            )?;
            if *r != expected {
                return Err("署名要求または表示情報の改変".into());
            }
            Ok(())
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn check_time(&mut self, clock: &impl Clock) -> Result<ClockReading> {
        let result = (|| {
            let now = clock.now()?;
            let checked = self.checked.as_ref().ok_or("検証結果欠損")?;
            if now.unix_ms >= checked.report.expires_at
                || now
                    .monotonic_ms
                    .checked_sub(checked.checked_clock.monotonic_ms)
                    .is_some_and(|n| n >= checked.ttl_ms)
            {
                self.state = State::Expired;
            }
            checked.validate_at(now)?;
            if self
                .request
                .as_ref()
                .is_some_and(|r| now.unix_ms < r.created_at)
            {
                return Err("要求作成前への時計巻き戻り".into());
            }
            Ok(now)
        })();
        if result.is_err() {
            self.reject();
        }
        result
    }
    fn reject(&mut self) {
        if !matches!(self.state, State::Signed | State::Expired) {
            self.state = State::Rejected;
        }
        self.checked = None;
    }
    fn fail<T>(&mut self, error: &'static str) -> Result<T> {
        self.reject();
        Err(error.into())
    }
}

fn request_from_checked(
    checked: &CheckedTransfer,
    id: &str,
    created_at: i64,
) -> Result<SignatureRequest> {
    let bytes = checked.signing_message();
    let decoded: VersionedMessage = bincode::deserialize(bytes)?;
    let VersionedMessage::Legacy(m) = decoded else {
        return Err("未対応メッセージ".into());
    };
    if m.instructions.len() != 1 || m.header.num_required_signatures != 1 {
        return Err("未対応命令構造".into());
    }
    let ix = &m.instructions[0];
    if ix.accounts.len() != 2
        || ix.data.len() != 12
        || ix.data[..4] != 2u32.to_le_bytes()
        || m.account_keys
            .get(ix.program_id_index as usize)
            .map(ToString::to_string)
            .as_deref()
            != Some("11111111111111111111111111111111")
    {
        return Err("transfer命令不一致".into());
    }
    let from = *m
        .account_keys
        .get(ix.accounts[0] as usize)
        .ok_or("送金元欠損")?;
    let to = *m
        .account_keys
        .get(ix.accounts[1] as usize)
        .ok_or("送金先欠損")?;
    let amount = u64::from_le_bytes(ix.data[4..12].try_into()?);
    let report = checked.report();
    if from != checked.transfer.from
        || amount != TRANSFER_AMOUNT
        || from == to
        || m.config_hash_prefix != report.network_config_prefix
        || signing_message_hash(bytes) != report.signing_message_hash
        || created_at < report.checked_at
        || created_at >= report.expires_at
        || report.estimated_fee_kelvin > report.estimated_fee_limit_kelvin
        || crate::transfer_builder(from, to, m.valid_from, m.config_hash_prefix).build()? != bytes
    {
        return Err("署名対象と表示情報の不整合".into());
    }
    Ok(SignatureRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: id.to_owned(),
        created_at,
        expires_at: report.expires_at,
        network: "testnet".into(),
        rpc_endpoint: TESTNET_RPC.into(),
        network_config_prefix: m.config_hash_prefix,
        sender_address: from.to_string(),
        recipient_address: to.to_string(),
        amount_kelvin: amount,
        estimated_fee_kelvin: report.estimated_fee_kelvin,
        estimated_fee_limit_kelvin: report.estimated_fee_limit_kelvin,
        signing_message: bytes.to_vec(),
        signing_message_hash: report.signing_message_hash,
        expected_public_key: from.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::cell::Cell;
    struct FakeClock {
        ms: Cell<i64>,
    }
    impl Clock for FakeClock {
        fn now(&self) -> Result<ClockReading> {
            Ok(ClockReading {
                unix_ms: self.ms.get(),
                monotonic_ms: self.ms.get() as u64,
            })
        }
    }
    struct Mock {
        prefix: u64,
        fee: u64,
        calls: Cell<u32>,
        fail: bool,
    }
    impl ReadRpc for Mock {
        async fn prefix(&self) -> Result<ConfigHashPrefix> {
            self.calls.set(self.calls.get() + 1);
            if self.fail {
                return Err("mock RPC failure".into());
            }
            Ok(ConfigHashPrefix::new(self.prefix))
        }
        async fn fee(&self, _: &str) -> Result<Option<u64>> {
            Ok(Some(self.fee))
        }
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            Ok(2_000_000)
        }
    }
    async fn created() -> (SignatureLifecycle, SigningKey, FakeClock, Mock) {
        // Synthetic key lives only in this test process; never printed or persisted.
        let key = SigningKey::from_bytes(&[73; 32]);
        let from = Pubkey::from(key.verifying_key().to_bytes()).to_string();
        let to = Pubkey::from([2; 32]).to_string();
        let transfer =
            PreparedTransfer::new(TESTNET_RPC, &from, &to, 1234, ConfigHashPrefix::new(42))
                .unwrap();
        let hash = signing_message_hash(transfer.signing_message());
        let clock = FakeClock {
            ms: Cell::new(10_000),
        };
        let rpc = Mock {
            prefix: 42,
            fee: 5000,
            calls: Cell::new(0),
            fail: false,
        };
        let checked = check_with_rpc_and_clock(transfer, hash, 6000, &rpc, 5000, &clock)
            .await
            .unwrap();
        (SignatureLifecycle::new(checked).unwrap(), key, clock, rpc)
    }
    async fn validate(
        session: &mut SignatureLifecycle,
        clock: &impl Clock,
        rpc: &Mock,
    ) -> Result<()> {
        session
            .validate_using(clock, |c| revalidate_with_rpc(c, rpc, clock))
            .await
    }
    async fn awaiting() -> (SignatureLifecycle, SigningKey, FakeClock, SignatureRequest) {
        let (mut s, key, clock, rpc) = created().await;
        assert_eq!(s.state(), State::Created);
        validate(&mut s, &clock, &rpc).await.unwrap();
        assert_eq!(s.state(), State::Validated);
        assert_eq!(rpc.calls.get(), 4); // Initial and fresh revalidation each query prefix twice.
        let request = s.begin_signing(&clock).unwrap();
        assert_eq!(s.state(), State::AwaitingSignature);
        (s, key, clock, request)
    }
    fn response(request: &SignatureRequest, key: &SigningKey) -> SignatureResponse {
        SignatureResponse {
            protocol_version: PROTOCOL_VERSION,
            request_id: request.request_id.clone(),
            public_key: request.expected_public_key.clone(),
            signature: key.sign(&request.signing_message).to_bytes().to_vec(),
            signing_message_hash: request.signing_message_hash,
        }
    }
    #[tokio::test]
    async fn valid_request_fields_come_from_message_and_checks() {
        let (_, key, _, r) = awaiting().await;
        assert_eq!(r.protocol_version, 1);
        assert_eq!(r.created_at, 10000);
        assert_eq!(r.expires_at, 15000);
        assert_eq!(
            r.sender_address,
            Pubkey::from(key.verifying_key().to_bytes()).to_string()
        );
        assert_eq!(r.expected_public_key, r.sender_address);
        assert_eq!(r.recipient_address, Pubkey::from([2; 32]).to_string());
        assert_eq!(r.amount_kelvin, TRANSFER_AMOUNT);
        assert_eq!(r.estimated_fee_kelvin, 5000);
        assert_eq!(r.estimated_fee_limit_kelvin, 6000);
        assert_eq!(r.network_config_prefix, ConfigHashPrefix::new(42));
        assert_eq!(r.network, "testnet");
        assert_eq!(r.rpc_endpoint, TESTNET_RPC);
        assert_eq!(
            r.signing_message_hash,
            signing_message_hash(&r.signing_message)
        );
    }
    #[tokio::test]
    async fn valid_response_builds_local_wire_and_signature_verifies() {
        let (mut s, key, clock, r) = awaiting().await;
        let resp = response(&r, &key);
        key.verifying_key()
            .verify_strict(
                &r.signing_message,
                &ed25519_dalek::Signature::from_slice(&resp.signature).unwrap(),
            )
            .unwrap();
        let wire = s.accept_response(resp, &clock).unwrap();
        assert_eq!(s.state(), State::Signed);
        let tx = rialo_cdk::transaction::builder::Transaction::deserialize_wired(&wire).unwrap();
        assert_eq!(tx.message(), r.signing_message);
        assert_eq!(tx.signature_count(), 1);
        key.verifying_key()
            .verify_strict(
                tx.message(),
                &ed25519_dalek::Signature::from_slice(&tx.signatures[0]).unwrap(),
            )
            .unwrap();
    }
    macro_rules! bad_response {
        ($name:ident, $change:expr) => {
            #[tokio::test]
            async fn $name() {
                let (mut s, key, clock, r) = awaiting().await;
                let mut resp = response(&r, &key);
                ($change)(&mut resp);
                assert!(s.accept_response(resp, &clock).is_err());
                assert_eq!(s.state(), State::Rejected);
                assert!(s.accept_response(response(&r, &key), &clock).is_err());
            }
        };
    }
    bad_response!(rejects_invalid_signature, |r: &mut SignatureResponse| r
        .signature[0] ^=
        1);
    bad_response!(rejects_public_key_mismatch, |r: &mut SignatureResponse| r
        .public_key =
        Pubkey::from([3; 32]).to_string());
    bad_response!(rejects_request_id_mismatch, |r: &mut SignatureResponse| r
        .request_id
        .push('x'));
    bad_response!(
        rejects_protocol_version_mismatch,
        |r: &mut SignatureResponse| r.protocol_version = 2
    );
    bad_response!(rejects_hash_mismatch, |r: &mut SignatureResponse| r
        .signing_message_hash[0] ^=
        1);
    bad_response!(rejects_missing_signature, |r: &mut SignatureResponse| r
        .signature
        .clear());
    macro_rules! bad_request {
        ($name:ident, $change:expr) => {
            #[tokio::test]
            async fn $name() {
                let (mut s, key, clock, r) = awaiting().await;
                ($change)(s.request.as_mut().unwrap());
                assert!(s.accept_response(response(&r, &key), &clock).is_err());
                assert_eq!(s.state(), State::Rejected);
            }
        };
    }
    bad_request!(rejects_message_tampering, |r: &mut SignatureRequest| r
        .signing_message[0] ^=
        1);
    bad_request!(rejects_recipient_tampering, |r: &mut SignatureRequest| r
        .recipient_address =
        Pubkey::from([3; 32]).to_string());
    bad_request!(rejects_amount_tampering, |r: &mut SignatureRequest| r
        .amount_kelvin +=
        1);
    bad_request!(rejects_network_tampering, |r: &mut SignatureRequest| r
        .network_config_prefix =
        ConfigHashPrefix::new(99));
    bad_request!(rejects_fee_display_tampering, |r: &mut SignatureRequest| {
        r.estimated_fee_kelvin = 1
    });
    #[tokio::test]
    async fn rejects_expired_response_and_reuse() {
        let (mut s, key, clock, r) = awaiting().await;
        clock.ms.set(r.expires_at);
        assert!(s.accept_response(response(&r, &key), &clock).is_err());
        assert_eq!(s.state(), State::Expired);
        assert!(s.begin_signing(&clock).is_err());
        assert!(s.accept_response(response(&r, &key), &clock).is_err());
        assert_eq!(s.state(), State::Expired);
    }
    #[tokio::test]
    async fn rejects_double_response_and_signed_request_reuse() {
        let (mut s, key, clock, r) = awaiting().await;
        s.accept_response(response(&r, &key), &clock).unwrap();
        assert!(s.accept_response(response(&r, &key), &clock).is_err());
        assert!(s.begin_signing(&clock).is_err());
        assert_eq!(s.state(), State::Signed);
    }
    #[tokio::test]
    async fn rejects_rejected_session_revalidation() {
        let (mut s, _, clock, rpc) = created().await;
        assert!(s.begin_signing(&clock).is_err()); // Cannot bypass validation.
        assert_eq!(s.state(), State::Rejected);
        let before = rpc.calls.get();
        assert!(validate(&mut s, &clock, &rpc).await.is_err());
        assert_eq!(rpc.calls.get(), before);
    }
    #[tokio::test]
    async fn network_mismatch_blocks_request_creation() {
        let (mut s, _, clock, mut rpc) = created().await;
        rpc.prefix = 99;
        assert!(validate(&mut s, &clock, &rpc).await.is_err());
        assert!(s.request.is_none());
        assert_eq!(s.state(), State::Rejected);
    }
    #[tokio::test]
    async fn fee_excess_blocks_request_creation() {
        let (mut s, _, clock, mut rpc) = created().await;
        rpc.fee = 6001;
        assert!(validate(&mut s, &clock, &rpc).await.is_err());
        assert!(s.request.is_none());
        assert_eq!(s.state(), State::Rejected);
    }
    #[tokio::test]
    async fn rpc_failure_blocks_request_creation() {
        let (mut s, _, clock, mut rpc) = created().await;
        rpc.fail = true;
        assert!(validate(&mut s, &clock, &rpc).await.is_err());
        assert_eq!(s.state(), State::Rejected);
    }
    #[tokio::test]
    async fn expired_checks_block_request_creation() {
        let (mut s, _, clock, rpc) = created().await;
        clock.ms.set(15000);
        assert!(validate(&mut s, &clock, &rpc).await.is_err());
        assert_eq!(s.state(), State::Expired);
        assert!(s.request.is_none());
    }
    #[tokio::test]
    async fn clock_failure_rejects_response() {
        struct BadClock;
        impl Clock for BadClock {
            fn now(&self) -> Result<ClockReading> {
                Err("clock failure".into())
            }
        }
        let (mut s, key, _, r) = awaiting().await;
        assert!(s.accept_response(response(&r, &key), &BadClock).is_err());
        assert_eq!(s.state(), State::Rejected);
    }
    #[tokio::test]
    async fn expiry_during_assembly_never_returns_wire() {
        struct StepClock(Cell<i64>);
        impl Clock for StepClock {
            fn now(&self) -> Result<ClockReading> {
                let n = self.0.get();
                self.0.set(15000);
                Ok(ClockReading {
                    unix_ms: n,
                    monotonic_ms: n as u64,
                })
            }
        }
        let (mut s, key, _, r) = awaiting().await;
        assert!(
            s.accept_response(response(&r, &key), &StepClock(Cell::new(14999)))
                .is_err()
        );
        assert_eq!(s.state(), State::Expired);
    }
    #[tokio::test]
    async fn cancelled_validation_is_terminal() {
        let (mut s, _, clock, _) = created().await;
        let mut future = Box::pin(s.validate_using(&clock, |_| std::future::pending()));
        // Poll once, then cancel without clocks, sleeps, network or spawning.
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(future.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(future);
        assert_eq!(s.state(), State::Rejected);
        assert!(s.begin_signing(&clock).is_err());
    }
    #[tokio::test]
    async fn ids_are_distinct_within_process() {
        let (a, _, _, _) = created().await;
        let (b, _, _, _) = created().await;
        assert_ne!(a.id, b.id);
    }
}
