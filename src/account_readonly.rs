//! Public account inspection. Raw account data is never part of a serializable report.
use base64::{Engine, engine::general_purpose::STANDARD};
use rialo_cdk::{
    error::RialoError,
    rpc::{
        HttpRpcClient,
        types::{AccountInfoResponse, DataSliceConfig, GetAccountInfoConfig, Pubkey},
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{str::FromStr, time::Duration};

pub const MAX_DATA_BYTES: usize = 4096;

/// Fixed reasons and numeric codes only; no response body or provider message.
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountError {
    InvalidInput,
    Timeout,
    Transport,
    Http { status: u16, code: i64 },
    Rpc { code: i64 },
    MalformedResponse,
    UnsupportedEncoding,
    DataLimit,
}
impl From<RialoError> for AccountError {
    fn from(error: RialoError) -> Self {
        match error {
            RialoError::Rpc(details) => match details.status {
                Some(status) if status != 200 => Self::Http {
                    status,
                    code: details.code,
                },
                _ => Self::Rpc { code: details.code },
            },
            RialoError::Network(error) if error.is_timeout() => Self::Timeout,
            RialoError::Json(_) => Self::MalformedResponse,
            _ => Self::Transport,
        }
    }
}
impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "account query failed ({self:?})")
    }
}
impl std::error::Error for AccountError {}

#[allow(async_fn_in_trait)]
pub trait AccountRpc {
    async fn account(&self, address: &Pubkey) -> Result<Value, AccountError>;
}
impl AccountRpc for HttpRpcClient {
    async fn account(&self, address: &Pubkey) -> Result<Value, AccountError> {
        self.call_with_json("getAccountInfo", request_params(address))
            .await
            .map_err(AccountError::from)
    }
}
fn request_params(address: &Pubkey) -> Value {
    let config = GetAccountInfoConfig {
        data_slice: Some(DataSliceConfig {
            offset: 0,
            length: MAX_DATA_BYTES as u64,
        }),
        min_context_slot: None,
    };
    serde_json::json!([{
            "version": 0, "address": address.to_string(), "config": config
    }])
}

#[derive(Debug, Serialize)]
pub struct DataSummary {
    pub encoding: &'static str,
    pub returned_bytes: usize,
    pub requested_offset: usize,
    pub requested_max_bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct PublicAccount {
    pub owner: String,
    pub executable: bool,
    pub balance_kelvin: u64,
    pub rent_epoch: u64,
    pub space: usize,
    pub data: DataSummary,
}
#[derive(Debug, Serialize)]
pub struct AccountReport {
    pub address: String,
    pub context: Value,
    pub status: &'static str,
    pub account: Option<PublicAccount>,
}

/// Deliberately has no Serialize or Debug implementation. Only report is for logging.
pub struct AccountObservation {
    pub report: AccountReport,
    data: Vec<u8>,
}
impl AccountObservation {
    /// Bounded slice for explicit in-memory analysis; never automatically printed.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

pub fn validate(address: &str, timeout: Duration) -> Result<Pubkey, AccountError> {
    if !(Duration::from_secs(1)..=Duration::from_secs(30)).contains(&timeout) {
        return Err(AccountError::InvalidInput);
    }
    Pubkey::from_str(address).map_err(|_| AccountError::InvalidInput)
}

pub async fn query(
    rpc: &impl AccountRpc,
    address: &str,
    timeout: Duration,
) -> Result<AccountObservation, AccountError> {
    let address = validate(address, timeout)?;
    tokio::time::timeout(timeout, async {
        let raw = rpc.account(&address).await?;
        // Optional value's absence must not masquerade as an explicit null account.
        if !raw
            .as_object()
            .is_some_and(|object| object.contains_key("value"))
        {
            return Err(AccountError::MalformedResponse);
        }
        let response: AccountInfoResponse =
            serde_json::from_value(raw).map_err(|_| AccountError::MalformedResponse)?;
        let context =
            serde_json::to_value(response.context).map_err(|_| AccountError::MalformedResponse)?;
        let Some(account) = response.value else {
            return Ok(AccountObservation {
                report: AccountReport {
                    address: address.to_string(),
                    context,
                    status: "not_found",
                    account: None,
                },
                data: vec![],
            });
        };
        let owner =
            Pubkey::from_str(&account.owner).map_err(|_| AccountError::MalformedResponse)?;
        if account.data.len() != 2 {
            return Err(AccountError::MalformedResponse);
        }
        if account.data[1] != "base64" {
            return Err(AccountError::UnsupportedEncoding);
        }
        if account.data[0].len() > MAX_DATA_BYTES.div_ceil(3) * 4 {
            return Err(AccountError::DataLimit);
        }
        let data = STANDARD
            .decode(&account.data[0])
            .map_err(|_| AccountError::MalformedResponse)?;
        if data.len() > MAX_DATA_BYTES {
            return Err(AccountError::DataLimit);
        }
        let report = AccountReport {
            address: address.to_string(),
            context,
            status: "present",
            account: Some(PublicAccount {
                owner: owner.to_string(),
                executable: account.executable,
                balance_kelvin: account.kelvin,
                rent_epoch: account.rent_epoch,
                space: account.space,
                data: DataSummary {
                    encoding: "base64",
                    returned_bytes: data.len(),
                    requested_offset: 0,
                    requested_max_bytes: MAX_DATA_BYTES,
                },
            }),
        };
        Ok(AccountObservation { report, data })
    })
    .await
    .map_err(|_| AccountError::Timeout)?
}

pub struct Options {
    pub address: String,
    pub timeout: Duration,
    pub execute: bool,
}
pub fn parse(args: &[String]) -> Result<Options, AccountError> {
    let mut address = None;
    let mut timeout = None;
    let mut execute = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--address" if address.is_none() => {
                i += 1;
                address = Some(args.get(i).ok_or(AccountError::InvalidInput)?.clone());
            }
            "--timeout-secs" if timeout.is_none() => {
                i += 1;
                timeout = Some(Duration::from_secs(
                    args.get(i)
                        .ok_or(AccountError::InvalidInput)?
                        .parse()
                        .map_err(|_| AccountError::InvalidInput)?,
                ));
            }
            "--execute" if !execute => execute = true,
            _ => return Err(AccountError::InvalidInput),
        }
        i += 1;
    }
    let address = address.ok_or(AccountError::InvalidInput)?;
    let timeout = timeout.unwrap_or(Duration::from_secs(10));
    validate(&address, timeout)?;
    Ok(Options {
        address,
        timeout,
        execute,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_request_uses_versioned_address_and_bounded_slice() {
        let address = Pubkey::from([1; 32]);
        assert_eq!(
            request_params(&address),
            serde_json::json!([{
                "version":0, "address":address.to_string(),
                "config":{"dataSlice":{"offset":0,"length":4096},"minContextSlot":null}
            }])
        );
    }
}
