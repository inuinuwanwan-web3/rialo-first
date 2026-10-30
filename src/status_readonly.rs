//! One bounded status read, retaining official per-transaction slots and typed errors.
//! No confirmation depth, finality, absence, or ledger-continuity inference.
use crate::{
    Result,
    monitor::{TransactionObservation, observe_status},
};
use rialo_cdk::rpc::{
    HttpRpcClient,
    types::{
        GetSignatureStatusesRequest, GetSignatureStatusesResponse, Signature, SignatureStatus,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, str::FromStr, time::Duration};

/// The transport has one fixed read operation; callers cannot supply an RPC method.
#[allow(async_fn_in_trait)]
pub trait StatusRpc {
    async fn signature_statuses(&self, request: GetSignatureStatusesRequest) -> Result<Value>;
}
impl StatusRpc for HttpRpcClient {
    async fn signature_statuses(&self, request: GetSignatureStatusesRequest) -> Result<Value> {
        Ok(self
            .call_with_json("getSignatureStatuses", serde_json::json!([request]))
            .await?)
    }
}

#[derive(Serialize)]
pub struct StatusReport {
    /// Official response, including distinct context and individual slots.
    pub response: GetSignatureStatusesResponse,
    /// Input-order association; classification reuses the existing monitor.
    pub transactions: Vec<TransactionObservation>,
}

/// Validate before constructing a live client. Duplicates are rejected, not reordered.
pub fn validate(signatures: &[String], timeout: Duration) -> Result<Vec<Signature>> {
    if !(1..=256).contains(&signatures.len()) {
        return Err("status query requires 1..256 signatures".into());
    }
    if timeout < Duration::from_secs(1) || timeout > Duration::from_secs(30) {
        return Err("status timeout must be 1..30 seconds".into());
    }
    let mut seen = HashSet::new();
    signatures
        .iter()
        .map(|text| {
            let id = Signature::from_str(text).map_err(|_| "invalid transaction signature")?;
            if !seen.insert(id) {
                return Err("duplicate transaction signature".into());
            }
            Ok(id)
        })
        .collect()
}

/// Exactly one request, no retries. Malformed/null/short results are errors, not absence.
pub async fn query(
    rpc: &impl StatusRpc,
    signatures: &[String],
    timeout: Duration,
) -> Result<StatusReport> {
    let ids = validate(signatures, timeout)?;
    tokio::time::timeout(timeout, async {
        let request = GetSignatureStatusesRequest {
            version: 0,
            signatures: ids.iter().map(ToString::to_string).collect(),
            config: None,
        };
        let response: GetSignatureStatusesResponse =
            serde_json::from_value(rpc.signature_statuses(request).await?)?;
        if response.version != 0 || response.value.len() != ids.len() {
            return Err("unsupported status version or response count mismatch".into());
        }
        let transactions = ids
            .iter()
            .zip(&response.value)
            .map(|(id, status)| {
                observe_status(
                    id,
                    Some(SignatureStatus {
                        slot: status.slot,
                        executed: status.executed,
                        err: status.err.as_ref().map(ToString::to_string),
                    }),
                )
            })
            .collect();
        Ok(StatusReport {
            response,
            transactions,
        })
    })
    .await
    .map_err(|_| "status RPC timeout")?
}

/// New standalone CLI only; existing monitor and Journal interfaces are unchanged.
pub struct Options {
    pub signatures: Vec<String>,
    pub timeout: Duration,
    pub execute: bool,
}
pub fn parse(args: &[String]) -> Result<Options> {
    let mut options = Options {
        signatures: vec![],
        timeout: Duration::from_secs(10),
        execute: false,
    };
    let mut timeout_seen = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--signature" => {
                i += 1;
                options
                    .signatures
                    .push(args.get(i).ok_or("missing signature")?.clone());
            }
            "--timeout-secs" if !timeout_seen => {
                timeout_seen = true;
                i += 1;
                let seconds: u64 = args
                    .get(i)
                    .ok_or("missing timeout")?
                    .parse()
                    .map_err(|_| "invalid timeout")?;
                options.timeout = Duration::from_secs(seconds);
            }
            "--execute" if !options.execute => options.execute = true,
            _ => return Err("unsupported or duplicate status option".into()),
        }
        i += 1;
    }
    validate(&options.signatures, options.timeout)?;
    Ok(options)
}
