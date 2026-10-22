//! Read-only, single-snapshot monitor. No wallet, signing, sending or polling.
pub mod cli;
pub mod query;
use crate::Result;
use rialo_cdk::{
    RpcClient,
    rpc::{
        HttpRpcClient,
        types::{Pubkey, Signature, SignatureStatus},
    },
};
use serde::Serialize;
use std::{str::FromStr, time::Duration};

/// Only the three read operations needed by the monitor are exposed.
#[allow(async_fn_in_trait)]
pub trait MonitorRpc {
    async fn balance(&self, address: &Pubkey) -> Result<u64>;
    async fn height(&self) -> Result<u64>;
    async fn statuses(&self, signatures: &[Signature]) -> Result<Vec<Option<SignatureStatus>>>;
}
impl MonitorRpc for HttpRpcClient {
    async fn balance(&self, address: &Pubkey) -> Result<u64> {
        Ok(self.get_balance(address).await?)
    }
    async fn height(&self) -> Result<u64> {
        Ok(self.get_block_height().await?)
    }
    async fn statuses(&self, signatures: &[Signature]) -> Result<Vec<Option<SignatureStatus>>> {
        Ok(self.get_signature_statuses(signatures).await?)
    }
}
#[derive(Debug, PartialEq, Serialize)]
pub enum Execution {
    Succeeded,
    Failed,
    Unconfirmed,
    Unknown,
}
/// Describes SDK evidence, without asserting presence or pending status.
#[derive(Debug, PartialEq, Serialize)]
pub enum StatusReason {
    ExecutedWithoutError,
    RpcReportedError,
    NoStatusReturned,
    UnexecutedOrNotFound,
}
#[derive(Debug, Serialize)]
pub struct TransactionObservation {
    pub signature: String,
    pub execution: Execution,
    pub status_reason: StatusReason,
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub address: String,
    pub balance_kelvin: u64,
    pub block_height: u64,
    pub transactions: Vec<TransactionObservation>,
    pub address_history_supported: bool,
    #[serde(skip_serializing)]
    pub confirmation_depth: Option<u64>,
}
pub(super) fn validate_request(
    address: &str,
    tx_ids: &[String],
) -> Result<(Pubkey, Vec<Signature>)> {
    let address = Pubkey::from_str(address).map_err(|_| "invalid public address")?;
    let mut signatures = Vec::new();
    for id in tx_ids {
        let signature = Signature::from_str(id).map_err(|_| "invalid transaction signature")?;
        if !signatures.contains(&signature) {
            signatures.push(signature);
        }
    }
    if signatures.len() > 256 {
        return Err("at most 256 distinct signatures".into());
    }
    Ok((address, signatures))
}
/// Classifies SDK evidence without inferring pending, absence or finality.
pub fn observe_status(
    signature: &Signature,
    status: Option<SignatureStatus>,
) -> TransactionObservation {
    let (execution, status_reason) = match status {
        None => (Execution::Unknown, StatusReason::NoStatusReturned),
        Some(s) if s.err.is_some() => (Execution::Failed, StatusReason::RpcReportedError),
        Some(s) if s.executed => (Execution::Succeeded, StatusReason::ExecutedWithoutError),
        Some(_) => (Execution::Unknown, StatusReason::UnexecutedOrNotFound),
    };
    TransactionObservation {
        signature: signature.to_string(),
        execution,
        status_reason,
    }
}
/// Validates all inputs before any reads; deduplicates TX IDs in input order.
/// SDK IDs are base58 signatures, not arbitrary hex hashes. Status absence is unknown.
/// Height and status are separate observations, not an atomic chain snapshot.
pub async fn snapshot(
    rpc: &impl MonitorRpc,
    address: &str,
    tx_ids: &[String],
    timeout: Duration,
) -> Result<Snapshot> {
    let (address, signatures) = validate_request(address, tx_ids)?;
    tokio::time::timeout(timeout, async {
        let block_height = rpc.height().await?;
        let balance_kelvin = rpc.balance(&address).await?;
        let statuses = if signatures.is_empty() {
            vec![]
        } else {
            rpc.statuses(&signatures).await?
        };
        if statuses.len() != signatures.len() {
            return Err("invalid status response length".into());
        }
        let transactions = signatures
            .iter()
            .zip(statuses)
            .map(|(signature, status)| observe_status(signature, status))
            .collect();
        Ok(Snapshot {
            address: address.to_string(),
            balance_kelvin,
            block_height,
            transactions,
            address_history_supported: false,
            confirmation_depth: None,
        })
    })
    .await
    .map_err(|_| "monitor RPC timeout")?
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Mock {
        balance: u64,
        executed: bool,
        failed: bool,
        mode: u8,
    }
    impl MonitorRpc for Mock {
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            match self.mode {
                1 => Err("synthetic RPC error".into()),
                2 => std::future::pending().await,
                _ => Ok(self.balance),
            }
        }
        async fn height(&self) -> Result<u64> {
            Ok(100)
        }
        async fn statuses(&self, ids: &[Signature]) -> Result<Vec<Option<SignatureStatus>>> {
            if self.mode == 4 {
                return Ok(vec![]);
            }
            Ok(ids
                .iter()
                .map(|_| {
                    if self.mode == 3 {
                        None
                    } else {
                        Some(SignatureStatus {
                            slot: 99,
                            executed: self.executed,
                            err: self.failed.then(|| "fixture failure".into()),
                        })
                    }
                })
                .collect())
        }
    }
    fn mock() -> Mock {
        Mock {
            balance: 42,
            executed: true,
            failed: false,
            mode: 0,
        }
    }
    fn address() -> String {
        Pubkey::from([1; 32]).to_string()
    }
    fn ids() -> Vec<String> {
        vec![Signature::from([1; 64]).to_string()]
    }
    async fn read(m: Mock) -> Result<Snapshot> {
        snapshot(&m, &address(), &ids(), Duration::from_millis(10)).await
    }
    #[tokio::test]
    async fn balance_normal() {
        assert_eq!(read(mock()).await.unwrap().balance_kelvin, 42);
    }
    #[tokio::test]
    async fn balance_zero() {
        assert_eq!(
            read(Mock {
                balance: 0,
                ..mock()
            })
            .await
            .unwrap()
            .balance_kelvin,
            0
        );
    }
    #[tokio::test]
    async fn tx_success() {
        assert_eq!(
            read(mock()).await.unwrap().transactions[0].execution,
            Execution::Succeeded
        );
    }
    #[tokio::test]
    async fn tx_failed() {
        assert_eq!(
            read(Mock {
                failed: true,
                ..mock()
            })
            .await
            .unwrap()
            .transactions[0]
                .execution,
            Execution::Failed
        );
    }
    #[tokio::test]
    async fn ambiguous_unexecuted_is_unknown() {
        assert_eq!(
            read(Mock {
                executed: false,
                ..mock()
            })
            .await
            .unwrap()
            .transactions[0]
                .execution,
            Execution::Unknown
        );
    }
    #[tokio::test]
    async fn tx_unknown() {
        assert_eq!(
            read(Mock { mode: 3, ..mock() }).await.unwrap().transactions[0].execution,
            Execution::Unknown
        );
    }
    #[tokio::test]
    async fn invalid_hash_before_rpc() {
        assert!(
            snapshot(
                &Mock { mode: 2, ..mock() },
                &address(),
                &["invalid".into()],
                Duration::from_secs(1)
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("invalid transaction")
        );
    }
    #[tokio::test]
    async fn rpc_timeout() {
        assert!(
            read(Mock { mode: 2, ..mock() })
                .await
                .unwrap_err()
                .to_string()
                .contains("timeout")
        );
    }
    #[tokio::test]
    async fn rpc_error() {
        assert!(read(Mock { mode: 1, ..mock() }).await.is_err());
    }
    #[tokio::test]
    async fn duplicate_tx() {
        let id = ids()[0].clone();
        assert_eq!(
            snapshot(
                &mock(),
                &address(),
                &[id.clone(), id],
                Duration::from_secs(1)
            )
            .await
            .unwrap()
            .transactions
            .len(),
            1
        );
    }
    #[tokio::test]
    async fn no_requested_tx_is_not_empty_history() {
        let s = snapshot(&mock(), &address(), &[], Duration::from_secs(1))
            .await
            .unwrap();
        assert!(s.transactions.is_empty());
        assert!(!s.address_history_supported);
        assert_eq!(s.confirmation_depth, None);
    }
    #[tokio::test]
    async fn malformed_status_count() {
        assert!(read(Mock { mode: 4, ..mock() }).await.is_err());
    }
    #[tokio::test]
    async fn unknown_reasons_preserve_ambiguity() {
        assert_eq!(
            read(Mock { mode: 3, ..mock() }).await.unwrap().transactions[0].status_reason,
            StatusReason::NoStatusReturned
        );
        assert_eq!(
            read(Mock {
                executed: false,
                ..mock()
            })
            .await
            .unwrap()
            .transactions[0]
                .status_reason,
            StatusReason::UnexecutedOrNotFound
        );
        assert_eq!(
            read(mock()).await.unwrap().transactions[0].status_reason,
            StatusReason::ExecutedWithoutError
        );
        assert_eq!(
            read(Mock {
                failed: true,
                ..mock()
            })
            .await
            .unwrap()
            .transactions[0]
                .status_reason,
            StatusReason::RpcReportedError
        );
    }
    #[test]
    fn recorded_live_status_remains_unknown() {
        // SDK-converted status from 2026-10-22; slot is context, not TX inclusion proof.
        let status: SignatureStatus =
            serde_json::from_str(r#"{"slot":1197107,"executed":false,"err":null}"#).unwrap();
        let observation = observe_status(&Signature::from([1; 64]), Some(status));
        assert_eq!(observation.execution, Execution::Unknown);
        assert_eq!(
            observation.status_reason,
            StatusReason::UnexecutedOrNotFound
        );
    }
}
