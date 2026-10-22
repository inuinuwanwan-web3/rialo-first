//! Opt-in, bounded queries using locally shipped official API schemas.
//! Live server compatibility and retention are not established by these schemas.
use super::validate_request;
use crate::Result;
use rialo_cdk::rpc::{
    HttpRpcClient,
    types::{
        GetSignaturesForAddressConfig, GetSignaturesForAddressRequest,
        GetSignaturesForAddressResponse, GetTransactionResponse, Signature, SignatureInfo,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{str::FromStr, time::Duration};

/// Restricts this adapter to the two reviewed read methods.
pub enum Query {
    History(GetSignaturesForAddressRequest),
    Detail(Signature),
}
impl Query {
    fn request(&self) -> Result<(&'static str, Value)> {
        Ok(match self {
            Self::History(request) => ("getSignaturesForAddress", serde_json::json!([request])),
            Self::Detail(signature) => (
                "getTransaction",
                serde_json::json!([{"version":0,"signature":signature.to_string()}]),
            ),
        })
    }
}
#[allow(async_fn_in_trait)]
pub trait QueryRpc {
    async fn query(&self, request: Query) -> Result<Value>;
}
impl QueryRpc for HttpRpcClient {
    async fn query(&self, request: Query) -> Result<Value> {
        let (method, params) = request.request()?;
        Ok(self.call_with_json(method, params).await?)
    }
}
#[derive(Debug, Serialize)]
pub struct HistoryPage {
    pub context_slot: u64,
    pub entries: Vec<SignatureInfo>,
    /// Candidate for an explicit next request, not a guarantee that another page exists.
    pub next_before: Option<String>,
    pub duplicates_removed: usize,
}
/// Fetches exactly one page. Never silently follows cursors or claims complete history.
pub async fn history_page(
    rpc: &impl QueryRpc,
    address: &str,
    config: GetSignaturesForAddressConfig,
    timeout: Duration,
) -> Result<HistoryPage> {
    let (address, _) = validate_request(address, &[])?;
    let limit = config.limit.unwrap_or(20);
    if !(1..=1000).contains(&limit) {
        return Err("history limit must be 1..1000".into());
    }
    for cursor in [&config.before, &config.until].into_iter().flatten() {
        Signature::from_str(cursor).map_err(|_| "invalid history cursor")?;
    }
    let before = config.before.clone();
    let request = GetSignaturesForAddressRequest {
        version: 0,
        address: address.to_string(),
        config: Some(config),
    };
    tokio::time::timeout(timeout, async {
        // Official context/value shape only; incompatible bare arrays are errors.
        let response: GetSignaturesForAddressResponse =
            serde_json::from_value(rpc.query(Query::History(request)).await?)?;
        if response.value.len() > usize::from(limit) {
            return Err("history response exceeds requested limit".into());
        }
        let next_before = if response.value.len() == usize::from(limit) {
            response.value.last().map(|e| e.signature.clone())
        } else {
            None
        };
        if next_before.is_some() && next_before == before {
            return Err("history cursor did not advance".into());
        }
        let mut entries: Vec<SignatureInfo> = Vec::new();
        let mut duplicates_removed = 0;
        for entry in response.value {
            Signature::from_str(&entry.signature)
                .map_err(|_| "invalid history response signature")?;
            if let Some(old) = entries.iter().find(|e| e.signature == entry.signature) {
                if serde_json::to_value(old)? != serde_json::to_value(&entry)? {
                    return Err("conflicting duplicate history entry".into());
                }
                duplicates_removed += 1;
            } else {
                entries.push(entry);
            }
        }
        Ok(HistoryPage {
            context_slot: response.context.slot,
            entries,
            next_before,
            duplicates_removed,
        })
    })
    .await
    .map_err(|_| "history RPC timeout")?
}
/// Returns the official detailed transaction type, with block height and execution metadata.
/// Null/malformed/error is not proof of absence and remains an error. No finality inference.
pub async fn transaction_detail(
    rpc: &impl QueryRpc,
    tx_id: &str,
    timeout: Duration,
) -> Result<GetTransactionResponse> {
    let signature = Signature::from_str(tx_id).map_err(|_| "invalid transaction signature")?;
    tokio::time::timeout(timeout, async {
        let response: GetTransactionResponse =
            serde_json::from_value(rpc.query(Query::Detail(signature)).await?)?;
        if response.transaction.signatures.first().map(String::as_str) != Some(tx_id) {
            return Err("transaction response ID mismatch".into());
        }
        Ok(response)
    })
    .await
    .map_err(|_| "transaction detail RPC timeout")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Mock {
        replies: RefCell<Vec<Value>>,
        seen: RefCell<Vec<Value>>,
        mode: u8,
    }
    impl QueryRpc for Mock {
        async fn query(&self, request: Query) -> Result<Value> {
            let (method, params) = request.request()?;
            self.seen
                .borrow_mut()
                .push(serde_json::json!({"method":method,"params":params}));
            match self.mode {
                1 => Err("fixture RPC error".into()),
                2 => std::future::pending().await,
                _ => Ok(self.replies.borrow_mut().remove(0)),
            }
        }
    }
    fn mock(replies: Vec<Value>) -> Mock {
        Mock {
            replies: RefCell::new(replies),
            seen: RefCell::new(vec![]),
            mode: 0,
        }
    }
    fn id(n: u8) -> String {
        Signature::from([n; 64]).to_string()
    }
    fn address() -> String {
        rialo_cdk::rpc::types::Pubkey::from([1; 32]).to_string()
    }
    fn config(limit: u16, before: Option<String>) -> GetSignaturesForAddressConfig {
        GetSignaturesForAddressConfig {
            limit: Some(limit),
            before,
            until: None,
        }
    }
    fn entry(n: u8) -> Value {
        serde_json::json!({"signature":id(n),"blockHeight":90,"err":null,"blockTime":null})
    }
    fn page(entries: Vec<Value>) -> Value {
        serde_json::json!({"context":{"slot":100},"value":entries})
    }
    fn detail(failed: bool) -> Value {
        serde_json::json!({"context":{"slot":100},"block_height":90,"block_time":null,
          "transaction":{"signatures":[id(1)],"validFrom":123,"message":{"accountKeys":[address()],"header":{"numRequiredSignatures":1,"numReadonlySignedAccounts":0,"numReadonlyUnsignedAccounts":0},"instructions":[]}},
          "meta":{"err":if failed {serde_json::json!({"fixture":"failure"})} else {Value::Null},"fee":5,"logMessages":null}})
    }
    const TIMEOUT: Duration = Duration::from_millis(10);
    #[tokio::test]
    async fn empty_history() {
        let m = mock(vec![page(vec![])]);
        let p = history_page(&m, &address(), config(20, None), TIMEOUT)
            .await
            .unwrap();
        assert!(p.entries.is_empty());
        assert!(p.next_before.is_none());
        assert_eq!(p.context_slot, 100);
    }
    #[tokio::test]
    async fn multiple_transactions() {
        let m = mock(vec![page(vec![entry(1), entry(2)])]);
        let p = history_page(&m, &address(), config(20, None), TIMEOUT)
            .await
            .unwrap();
        assert_eq!(p.entries.len(), 2);
        assert!(p.next_before.is_none());
    }
    #[tokio::test]
    async fn explicit_pagination_requests() {
        let m = mock(vec![page(vec![entry(1), entry(2)]), page(vec![])]);
        let p = history_page(&m, &address(), config(2, None), TIMEOUT)
            .await
            .unwrap();
        assert_eq!(p.next_before, Some(id(2)));
        let p2 = history_page(&m, &address(), config(2, p.next_before), TIMEOUT)
            .await
            .unwrap();
        assert!(p2.entries.is_empty());
        let seen = m.seen.borrow();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1]["method"], "getSignaturesForAddress");
        assert_eq!(seen[1]["params"][0]["config"]["before"], id(2));
        assert_eq!(seen[0]["params"][0]["version"], 0);
    }
    #[tokio::test]
    async fn duplicate_transaction() {
        let m = mock(vec![page(vec![entry(1), entry(1)])]);
        let p = history_page(&m, &address(), config(3, None), TIMEOUT)
            .await
            .unwrap();
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.duplicates_removed, 1);
    }
    #[tokio::test]
    async fn conflicting_duplicate_rejected() {
        let mut e = entry(1);
        e["blockHeight"] = serde_json::json!(91);
        assert!(
            history_page(
                &mock(vec![page(vec![entry(1), e])]),
                &address(),
                config(3, None),
                TIMEOUT
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn nonadvancing_cursor_rejected() {
        assert!(
            history_page(
                &mock(vec![page(vec![entry(1)])]),
                &address(),
                config(1, Some(id(1))),
                TIMEOUT
            )
            .await
            .is_err()
        );
    }
    #[tokio::test]
    async fn history_shape_and_count_rejected() {
        for value in [serde_json::json!([]), page(vec![entry(1), entry(2)])] {
            assert!(
                history_page(&mock(vec![value]), &address(), config(1, None), TIMEOUT)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn invalid_inputs_no_query() {
        let m = mock(vec![]);
        assert!(
            history_page(&m, "invalid", config(1, None), TIMEOUT)
                .await
                .is_err()
        );
        assert!(
            history_page(&m, &address(), config(0, None), TIMEOUT)
                .await
                .is_err()
        );
        assert!(
            history_page(&m, &address(), config(1001, None), TIMEOUT)
                .await
                .is_err()
        );
        assert!(
            history_page(&m, &address(), config(1, Some("invalid".into())), TIMEOUT)
                .await
                .is_err()
        );
        assert!(transaction_detail(&m, "invalid", TIMEOUT).await.is_err());
        assert!(m.seen.borrow().is_empty());
    }
    #[tokio::test]
    async fn rpc_errors_and_timeouts() {
        for mode in [1, 2] {
            let mut m = mock(vec![]);
            m.mode = mode;
            assert!(
                history_page(&m, &address(), config(1, None), TIMEOUT)
                    .await
                    .is_err()
            );
            assert!(transaction_detail(&m, &id(1), TIMEOUT).await.is_err());
        }
    }
    #[tokio::test]
    async fn detail_success_and_block_evidence() {
        let m = mock(vec![detail(false)]);
        let d = transaction_detail(&m, &id(1), TIMEOUT).await.unwrap();
        assert!(d.meta.err.is_none());
        assert_eq!(d.block_height, 90);
        assert_eq!(d.meta.fee, 5);
        assert_eq!(m.seen.borrow()[0]["method"], "getTransaction");
    }
    #[tokio::test]
    async fn detail_failure() {
        assert!(
            transaction_detail(&mock(vec![detail(true)]), &id(1), TIMEOUT)
                .await
                .unwrap()
                .meta
                .err
                .is_some()
        );
    }
    #[tokio::test]
    async fn null_is_not_absence_proof() {
        assert!(
            transaction_detail(&mock(vec![Value::Null]), &id(1), TIMEOUT)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn detail_id_mismatch() {
        assert!(
            transaction_detail(&mock(vec![detail(false)]), &id(2), TIMEOUT)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn finality_remains_unspecified() {
        let d = transaction_detail(&mock(vec![detail(false)]), &id(1), TIMEOUT)
            .await
            .unwrap();
        let v = serde_json::to_value(d).unwrap();
        assert!(v.get("finality").is_none());
        assert!(v.get("confirmations").is_none());
    }
    #[tokio::test]
    async fn recorded_live_empty_history_shape() {
        // 2026-10-22 official Testnet result; a result payload, not a JSON-RPC envelope.
        let recorded = serde_json::json!({"context":{"slot":1197108,"api_version":"0.23.0-alpha.0"},"value":[]});
        let page = history_page(&mock(vec![recorded]), &address(), config(2, None), TIMEOUT)
            .await
            .unwrap();
        assert_eq!(page.context_slot, 1197108);
        assert!(page.entries.is_empty());
        assert!(page.next_before.is_none());
        assert_eq!(page.duplicates_removed, 0);
    }
}
