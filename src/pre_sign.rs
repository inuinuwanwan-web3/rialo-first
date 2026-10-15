//! Read-only checks. No signing or sending; the public entry point is not wired to CLI.
use crate::{PreparedTransfer, Result, TESTNET_RPC, TRANSFER_AMOUNT};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rialo_cdk::{
    RpcClient,
    rpc::{HttpRpcClient, types::Pubkey},
};
use rialo_s_message::{ConfigHashPrefix, VersionedMessage};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub fn signing_message_hash(message: &[u8]) -> [u8; 32] {
    Sha256::digest(message).into()
}

/// Point-in-time observations, not a fee cap or independent chain identity proof.
#[derive(Debug, PartialEq)]
pub struct CheckReport {
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
}
impl CheckedTransfer {
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
    let rpc = TestnetReadRpc(HttpRpcClient::new(TESTNET_RPC.to_owned()));
    tokio::time::timeout(
        Duration::from_secs(30),
        check_with_rpc(
            transfer,
            reviewed_message_hash,
            estimated_fee_limit_kelvin,
            &rpc,
        ),
    )
    .await
    .map_err(|_| "署名前チェックのRPCタイムアウト")?
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

async fn check_with_rpc(
    transfer: PreparedTransfer,
    reviewed_hash: [u8; 32],
    limit: u64,
    rpc: &impl ReadRpc,
) -> Result<CheckedTransfer> {
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
    let total = TRANSFER_AMOUNT
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
    Ok(CheckedTransfer {
        transfer,
        report: CheckReport {
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
}
