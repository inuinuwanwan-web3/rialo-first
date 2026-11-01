//! Output-only adapters. No RPC client construction, credentials, persistence, or signing.
use crate::{
    account_readonly::{AccountError, AccountReport},
    bounded_http::HttpReceipt,
    monitor::{
        self, Snapshot,
        query::{HistoryBatch, HistoryPage},
    },
    status_readonly::StatusReport,
};
use rialo_cdk::{
    error::RialoError,
    rpc::types::{GetTransactionResponse, Pubkey, Signature, SignatureInfo, SignatureStatus},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: &str = "1.0";
pub const MAX_JSON_BYTES: usize = 1024 * 1024;
pub const MAX_ENTRIES: usize = 256;
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Devnet,
    Testnet,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    BalanceStatusSnapshot,
    TransactionHistory,
    TransactionDetail,
    TransactionStatus,
    AccountLookup,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Success,
    NotFound,
    RpcError,
    HttpError,
    Timeout,
    Blocked,
    Unverified,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Sdk,
    BoundedHttp,
    ProvidedData,
}
#[derive(Debug, PartialEq)]
pub enum ReportError {
    InvalidData,
    InvalidTimestamp,
    OutputLimit,
}

/// Only fixed classifications and numeric provider codes may reach an error report.
#[derive(Serialize)]
pub struct Failure {
    kind: Status,
    reason: &'static str,
    code: Option<i64>,
    http_status: Option<u16>,
}
impl Failure {
    pub fn blocked() -> Self {
        Self::fixed(Status::Blocked, "invalid_input")
    }
    pub fn timeout() -> Self {
        Self::fixed(Status::Timeout, "timeout")
    }
    pub fn rpc() -> Self {
        Self::fixed(Status::RpcError, "rpc_or_response_error")
    }
    fn fixed(kind: Status, reason: &'static str) -> Self {
        Self {
            kind,
            reason,
            code: None,
            http_status: None,
        }
    }
    pub fn account(error: &AccountError) -> Self {
        match error {
            AccountError::Rpc { code } => Self {
                kind: Status::RpcError,
                reason: "rpc_error",
                code: Some(*code),
                http_status: None,
            },
            AccountError::Http { status, code } => Self {
                kind: Status::HttpError,
                reason: "http_error",
                code: Some(*code),
                http_status: Some(*status),
            },
            AccountError::Timeout => Self::timeout(),
            AccountError::Transport => Self::fixed(Status::RpcError, "transport_error"),
            AccountError::ResponseTooLarge { .. } => {
                Self::fixed(Status::Blocked, "response_too_large")
            }
            AccountError::DataLimit => Self::fixed(Status::Blocked, "data_limit"),
            AccountError::MalformedResponse | AccountError::InvalidRpcEnvelope => {
                Self::fixed(Status::Blocked, "invalid_response")
            }
            AccountError::UnsupportedEncoding | AccountError::UnsupportedContentEncoding => {
                Self::fixed(Status::Blocked, "unsupported_encoding")
            }
            AccountError::Redirect { status } => Self {
                kind: Status::Blocked,
                reason: "redirect_rejected",
                code: None,
                http_status: Some(*status),
            },
            AccountError::InvalidInput | AccountError::MethodNotAllowed => Self::blocked(),
        }
    }
    pub fn error(error: &(dyn std::error::Error + 'static)) -> Self {
        if error.is::<crate::status_readonly::StatusTimeout>() {
            return Self::timeout();
        }
        if let Some(error) = error.downcast_ref::<AccountError>() {
            return Self::account(error);
        }
        if let Some(error) = error.downcast_ref::<RialoError>() {
            return match error {
                RialoError::Rpc(details) => Self {
                    kind: if details.status.is_some_and(|s| s != 200) {
                        Status::HttpError
                    } else {
                        Status::RpcError
                    },
                    reason: "rpc_error",
                    code: Some(details.code),
                    http_status: details.status,
                },
                RialoError::Network(e) if e.is_timeout() => Self::timeout(),
                _ => Self::rpc(),
            };
        }
        Self::rpc()
    }
}

/// No arbitrary provider body, Data bytes, account message, or free-form error is accepted.
#[derive(Serialize)]
pub struct Report {
    schema_version: &'static str,
    network: Network,
    operation: Operation,
    timestamp_utc: String,
    status: Status,
    context: Value,
    data: Value,
    error: Option<Failure>,
    transport: Transport,
    http: Option<Value>,
    query_attempted: bool,
    /// Query outcome is not an implementation-test or online-certification PASS.
    verification: &'static str,
}
impl Report {
    fn new(
        network: Network,
        operation: Operation,
        status: Status,
        slot: Option<u64>,
        data: Value,
        error: Option<Failure>,
        transport: Transport,
        attempted: bool,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            network,
            operation,
            timestamp_utc: utc_timestamp(time)?,
            status,
            context: json!({"slot":slot}),
            data,
            error,
            transport,
            http: None,
            query_attempted: attempted,
            verification: "not_asserted",
        })
    }
    pub fn failure(
        network: Network,
        operation: Operation,
        failure: Failure,
        transport: Transport,
        attempted: bool,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        Self::new(
            network,
            operation,
            failure.kind,
            None,
            Value::Null,
            Some(failure),
            transport,
            attempted,
            time,
        )
    }
    pub fn unverified(
        network: Network,
        operation: Operation,
        transport: Transport,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        Self::new(
            network,
            operation,
            Status::Unverified,
            None,
            json!({"executed":false}),
            None,
            transport,
            false,
            time,
        )
    }
    pub fn account(
        network: Network,
        input: &AccountReport,
        receipt: Option<&HttpReceipt>,
        transport: Transport,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        let address = public_address(&input.address)?;
        let slot = slot(&input.context)?;
        let (status, account) = match (input.status, input.account.as_ref()) {
            ("not_found", None) => (Status::NotFound, Value::Null),
            ("present", Some(account)) => {
                if account.data.encoding != "base64"
                    || account.data.returned_bytes > crate::account_readonly::MAX_DATA_BYTES
                {
                    return Err(ReportError::InvalidData);
                }
                (
                    Status::Success,
                    json!({"owner":public_address(&account.owner)?,"executable":account.executable,"balance_kelvin":account.balance_kelvin,"rent_epoch":account.rent_epoch,"space":account.space,"data":{"encoding":"base64","returned_bytes":account.data.returned_bytes,"requested_max_bytes":account.data.requested_max_bytes,"requested_offset":account.data.requested_offset}}),
                )
            }
            _ => return Err(ReportError::InvalidData),
        };
        let mut report = Self::new(
            network,
            Operation::AccountLookup,
            status,
            slot,
            json!({"address":address,"account":account}),
            None,
            transport,
            !matches!(transport, Transport::ProvidedData),
            time,
        )?;
        if let Some(receipt) = receipt {
            if !matches!(transport, Transport::BoundedHttp) || receipt.jsonrpc_version != "2.0" {
                return Err(ReportError::InvalidData);
            }
            report.http = Some(
                json!({"status":receipt.http_status,"body_bytes":receipt.body_bytes,"body_limit":receipt.body_limit,"request_id":receipt.request_id,"jsonrpc_version":"2.0"}),
            );
        }
        Ok(report)
    }
    pub fn monitor(
        network: Network,
        input: &Snapshot,
        config_prefix: Option<u64>,
        transport: Transport,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        let entries:Result<Vec<_>,_>=input.transactions.iter().take(MAX_ENTRIES).map(|tx|Ok(json!({"signature":public_signature(&tx.signature)?,"execution":tx.execution,"status_reason":tx.status_reason}))).collect();
        Self::new(
            network,
            Operation::BalanceStatusSnapshot,
            Status::Success,
            None,
            json!({"address":public_address(&input.address)?,"balance_kelvin":input.balance_kelvin,"block_height":input.block_height,"network_config_prefix":config_prefix,"transactions":entries?,"total_entries":input.transactions.len(),"truncated":input.transactions.len()>MAX_ENTRIES,"address_history_supported":input.address_history_supported}),
            None,
            transport,
            !matches!(transport, Transport::ProvidedData),
            time,
        )
    }
    pub fn statuses(
        network: Network,
        input: &StatusReport,
        transport: Transport,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        if input.response.value.len() != input.transactions.len() {
            return Err(ReportError::InvalidData);
        }
        let mut entries = vec![];
        for (status, tx) in input
            .response
            .value
            .iter()
            .zip(&input.transactions)
            .take(MAX_ENTRIES)
        {
            let signature =
                Signature::from_str(&tx.signature).map_err(|_| ReportError::InvalidData)?;
            let observation = monitor::observe_status(
                &signature,
                Some(SignatureStatus {
                    slot: status.slot,
                    executed: status.executed,
                    err: status.err.as_ref().map(|_| "reported_error".into()),
                }),
            );
            entries.push(json!({"signature":signature.to_string(),"slot":status.slot,"executed":status.executed,"error_present":status.err.is_some(),"execution":observation.execution,"status_reason":observation.status_reason}));
        }
        Self::new(
            network,
            Operation::TransactionStatus,
            Status::Success,
            Some(input.response.context.slot),
            json!({"transactions":entries,"total_entries":input.transactions.len(),"truncated":input.transactions.len()>MAX_ENTRIES}),
            None,
            transport,
            !matches!(transport, Transport::ProvidedData),
            time,
        )
    }
    pub fn history_page(
        network: Network,
        input: &HistoryPage,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        Self::history(
            network,
            &input.entries,
            input.next_before.as_deref(),
            Some(input.context_slot),
            Some(1),
            time,
        )
    }
    pub fn history_batch(
        network: Network,
        input: &HistoryBatch,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        Self::history(
            network,
            &input.entries,
            input.next_before.as_deref(),
            None,
            Some(input.pages_read),
            time,
        )
    }
    fn history(
        network: Network,
        entries: &[SignatureInfo],
        next: Option<&str>,
        context: Option<u64>,
        pages: Option<usize>,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        let rows:Result<Vec<_>,_>=entries.iter().take(MAX_ENTRIES).map(|entry|Ok(json!({"signature":public_signature(&entry.signature)?,"block_height":entry.block_height,"block_time":entry.block_time,"error_present":entry.err.is_some()}))).collect();
        let truncated = entries.len() > MAX_ENTRIES;
        let next = if truncated {
            None
        } else {
            next.map(public_signature).transpose()?
        };
        Self::new(
            network,
            Operation::TransactionHistory,
            Status::Success,
            context,
            json!({"entries":rows?,"total_entries":entries.len(),"truncated":truncated,"pages_read":pages,"next_before":next}),
            None,
            Transport::ProvidedData,
            false,
            time,
        )
    }
    pub fn detail(
        network: Network,
        input: &GetTransactionResponse,
        time: SystemTime,
    ) -> Result<Self, ReportError> {
        let signature = public_signature(
            input
                .transaction
                .signatures
                .first()
                .ok_or(ReportError::InvalidData)?,
        )?;
        Self::new(
            network,
            Operation::TransactionDetail,
            Status::Success,
            Some(input.context.slot),
            json!({"signature":signature,"block_height":input.block_height,"block_time":input.block_time,"error_present":input.meta.err.is_some()}),
            None,
            Transport::ProvidedData,
            false,
            time,
        )
    }
    fn render(&self, limit: usize) -> Result<String, ReportError> {
        let mut writer = LimitedWriter {
            bytes: vec![],
            limit,
        };
        serde_json::to_writer(&mut writer, self).map_err(|_| ReportError::OutputLimit)?;
        String::from_utf8(writer.bytes).map_err(|_| ReportError::InvalidData)
    }
}

pub struct Rendered {
    pub stdout: String,
    pub failed: bool,
}
/// Render one complete JSON document, or a small fixed failure document. No partial JSON.
pub fn finish(
    result: Result<Report, ReportError>,
    network: Network,
    operation: Operation,
) -> Rendered {
    let failed = result
        .as_ref()
        .map(|r| {
            !matches!(
                r.status,
                Status::Success | Status::NotFound | Status::Unverified
            )
        })
        .unwrap_or(true);
    match result.and_then(|report| report.render(MAX_JSON_BYTES)) {
        Ok(stdout) => Rendered { stdout, failed },
        Err(error) => {
            let reason = match error {
                ReportError::InvalidData => "invalid_report_data",
                ReportError::InvalidTimestamp => "invalid_timestamp",
                ReportError::OutputLimit => "output_limit",
            };
            let timestamp = utc_timestamp(SystemTime::now()).ok();
            Rendered {stdout:json!({"schema_version":SCHEMA_VERSION,"network":network,"operation":operation,"timestamp_utc":timestamp,"status":"blocked","context":{"slot":null},"data":null,"error":{"kind":"blocked","reason":reason,"code":null,"http_status":null},"transport":null,"http":null,"query_attempted":null,"verification":"not_asserted"}).to_string(),failed:true}
        }
    }
}
struct LimitedWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedWriter {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("report size limit"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn public_address(value: &str) -> Result<String, ReportError> {
    Pubkey::from_str(value)
        .map(|v| v.to_string())
        .map_err(|_| ReportError::InvalidData)
}
fn public_signature(value: &str) -> Result<String, ReportError> {
    Signature::from_str(value)
        .map(|v| v.to_string())
        .map_err(|_| ReportError::InvalidData)
}
fn slot(context: &Value) -> Result<Option<u64>, ReportError> {
    match context.get("slot") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or(ReportError::InvalidData),
    }
}

/// Gregorian UTC, milliseconds, years 1970..9999; no timezone or new dependencies.
pub fn utc_timestamp(time: SystemTime) -> Result<String, ReportError> {
    let elapsed = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ReportError::InvalidTimestamp)?;
    let seconds = elapsed.as_secs();
    if seconds > 253402300799 {
        return Err(ReportError::InvalidTimestamp);
    }
    let z = (seconds / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
        elapsed.subsec_millis()
    ))
}
pub fn split_json(args: &[String]) -> Result<(Vec<String>, bool), ReportError> {
    let count = args.iter().filter(|arg| arg.as_str() == "--json").count();
    if count > 1 {
        return Err(ReportError::InvalidData);
    }
    Ok((
        args.iter()
            .filter(|arg| arg.as_str() != "--json")
            .cloned()
            .collect(),
        count == 1,
    ))
}

#[cfg(test)]
mod tests;
