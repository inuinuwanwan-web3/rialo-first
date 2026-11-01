//! Opt-in, fixed-DevNet read-only transport. No SDK modifications or response logging.
use crate::account_readonly::{self, AccountError, AccountRpc, Options};
use reqwest::{Client, Url, header};
use rialo_cdk::{constants::URL_DEVNET, rpc::types::Pubkey};
use serde::Serialize;
use serde_json::Value;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const DEFAULT_BODY_LIMIT: usize = 1024 * 1024;
pub const MAX_BODY_LIMIT: usize = 8 * 1024 * 1024;
const READ_METHODS: &[&str] = &["getAccountInfo"];

#[derive(Clone)]
pub struct Limits {
    pub body_bytes: usize,
    pub connect: Duration,
    pub read: Duration,
    pub total: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            body_bytes: DEFAULT_BODY_LIMIT,
            connect: Duration::from_secs(3),
            read: Duration::from_secs(3),
            total: Duration::from_secs(10),
        }
    }
}
impl Limits {
    fn validate(&self) -> Result<(), AccountError> {
        if !(1..=MAX_BODY_LIMIT).contains(&self.body_bytes)
            || [self.connect, self.read, self.total]
                .iter()
                .any(|d| d.is_zero() || *d > Duration::from_secs(30))
        {
            return Err(AccountError::InvalidInput);
        }
        Ok(())
    }
}

/// Public construction is fixed to the official SDK DevNet URL, without credentials/proxies.
pub struct BoundedHttp {
    client: Client,
    endpoint: Url,
    limits: Limits,
    next_id: AtomicU64,
    receipt: Mutex<Option<HttpReceipt>>,
}
/// Numeric metadata only, recorded after a validated successful JSON-RPC result.
#[derive(Clone, Debug, Serialize)]
pub struct HttpReceipt {
    pub http_status: u16,
    pub body_bytes: usize,
    pub body_limit: usize,
    pub request_id: u64,
    pub jsonrpc_version: &'static str,
}
impl BoundedHttp {
    pub fn devnet(limits: Limits) -> Result<Self, AccountError> {
        Self::build(
            Url::parse(URL_DEVNET).map_err(|_| AccountError::InvalidInput)?,
            limits,
        )
    }
    fn build(endpoint: Url, limits: Limits) -> Result<Self, AccountError> {
        limits.validate()?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .http1_only()
            .pool_max_idle_per_host(0)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(limits.connect)
            .read_timeout(limits.read)
            .timeout(limits.total)
            .build()
            .map_err(map_http)?;
        Ok(Self {
            client,
            endpoint,
            limits,
            next_id: AtomicU64::new(1),
            receipt: Mutex::new(None),
        })
    }
    pub fn receipt(&self) -> Option<HttpReceipt> {
        self.receipt.lock().ok().and_then(|value| value.clone())
    }
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, AccountError> {
        if !READ_METHODS.contains(&method) {
            return Err(AccountError::MethodNotAllowed);
        }
        *self.receipt.lock().map_err(|_| AccountError::Transport)? = None;
        tokio::time::timeout(self.limits.total, self.receive(method, params))
            .await
            .map_err(|_| AccountError::Timeout)?
    }
    async fn receive(&self, method: &str, params: Value) -> Result<Value, AccountError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut response = self
            .client
            .post(self.endpoint.clone())
            .header(header::ACCEPT_ENCODING, "identity")
            .header(header::CONNECTION, "close")
            .json(&serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .send()
            .await
            .map_err(map_http)?;
        let status = response.status().as_u16();
        if response.status().is_redirection() {
            return Err(AccountError::Redirect { status });
        }
        for encoding in response.headers().get_all(header::CONTENT_ENCODING) {
            if !encoding
                .to_str()
                .is_ok_and(|s| s.trim().eq_ignore_ascii_case("identity"))
            {
                return Err(AccountError::UnsupportedContentEncoding);
            }
        }
        if let Some(length) = response.headers().get(header::CONTENT_LENGTH) {
            // Invalid framing is rejected, never trusted to size an unbounded allocation.
            let length = length
                .to_str()
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or(AccountError::Transport)?;
            if length > self.limits.body_bytes as u64 {
                return Err(AccountError::ResponseTooLarge {
                    limit: self.limits.body_bytes,
                });
            }
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_http)? {
            if chunk.len() > self.limits.body_bytes - body.len() {
                // No append of the offending chunk; dropping response cancels its body stream.
                return Err(AccountError::ResponseTooLarge {
                    limit: self.limits.body_bytes,
                });
            }
            body.extend_from_slice(&chunk);
        }
        if !(200..300).contains(&status) {
            // Error bodies have the same cap; never return/log provider-controlled text.
            let code = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|v| v.get("error")?.get("code")?.as_i64())
                .unwrap_or(0);
            return Err(AccountError::Http { status, code });
        }
        let body_bytes = body.len();
        let mut envelope: Value =
            serde_json::from_slice(&body).map_err(|_| AccountError::MalformedResponse)?;
        drop(body);
        let object = envelope
            .as_object_mut()
            .ok_or(AccountError::InvalidRpcEnvelope)?;
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || object.get("id").and_then(Value::as_u64) != Some(id)
        {
            return Err(AccountError::InvalidRpcEnvelope);
        }
        match (object.contains_key("result"), object.contains_key("error")) {
            (true, false) => {
                *self.receipt.lock().map_err(|_| AccountError::Transport)? = Some(HttpReceipt {
                    http_status: status,
                    body_bytes,
                    body_limit: self.limits.body_bytes,
                    request_id: id,
                    jsonrpc_version: "2.0",
                });
                Ok(object.remove("result").expect("checked result"))
            }
            (false, true) => {
                let error = object
                    .get("error")
                    .and_then(Value::as_object)
                    .ok_or(AccountError::InvalidRpcEnvelope)?;
                let code = error
                    .get("code")
                    .and_then(Value::as_i64)
                    .ok_or(AccountError::InvalidRpcEnvelope)?;
                if error.get("message").and_then(Value::as_str).is_none() {
                    return Err(AccountError::InvalidRpcEnvelope);
                }
                Err(AccountError::Rpc { code })
            }
            _ => Err(AccountError::InvalidRpcEnvelope),
        }
    }
}
fn map_http(error: reqwest::Error) -> AccountError {
    if error.is_timeout() {
        AccountError::Timeout
    } else {
        AccountError::Transport
    }
}
impl AccountRpc for BoundedHttp {
    async fn account(&self, address: &Pubkey) -> Result<Value, AccountError> {
        self.rpc("getAccountInfo", account_readonly::request_params(address))
            .await
    }
}

pub struct Selection {
    pub bounded: bool,
    pub body_bytes: usize,
}
/// Strip only opt-in transport flags, then reuse the established account CLI parser.
pub fn parse_cli(args: &[String]) -> Result<(Options, Selection), AccountError> {
    let mut bounded = false;
    let mut bytes = None;
    let mut rest = vec![];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bounded-http" if !bounded => bounded = true,
            "--max-body-bytes" if bytes.is_none() => {
                i += 1;
                bytes = Some(
                    args.get(i)
                        .ok_or(AccountError::InvalidInput)?
                        .parse::<usize>()
                        .map_err(|_| AccountError::InvalidInput)?,
                );
            }
            "--bounded-http" | "--max-body-bytes" => return Err(AccountError::InvalidInput),
            _ => rest.push(args[i].clone()),
        }
        i += 1;
    }
    if bytes.is_some() && !bounded {
        return Err(AccountError::InvalidInput);
    }
    let body_bytes = bytes.unwrap_or(DEFAULT_BODY_LIMIT);
    if !(1..=MAX_BODY_LIMIT).contains(&body_bytes) {
        return Err(AccountError::InvalidInput);
    }
    Ok((
        account_readonly::parse(&rest)?,
        Selection {
            bounded,
            body_bytes,
        },
    ))
}

#[cfg(test)]
mod tests;
