//! Synthetic public data and mocks only; no wallet, signing or HTTP calls.
use base64::{Engine, engine::general_purpose::STANDARD};
use rialo_cdk::{
    error::{RialoError, RpcErrorDetails},
    rpc::types::Pubkey,
};
use rialo_first::account_readonly::{self, AccountError, AccountRpc, MAX_DATA_BYTES};
use serde_json::{Value, json};
use std::{cell::Cell, time::Duration};
const TIME: Duration = Duration::from_secs(1);
fn address() -> String {
    Pubkey::from([1; 32]).to_string()
}
fn response(data: &[u8]) -> Value {
    json!({"context":{"slot":123,"api_version":"synthetic"},"value":{"kelvin":42,"owner":"11111111111111111111111111111111","executable":false,"data":[STANDARD.encode(data),"base64"],"space":9999,"rentEpoch":7}})
}
struct Mock {
    value: Value,
    calls: Cell<usize>,
    error: Option<AccountError>,
    dropped: Cell<bool>,
    pending: bool,
}
impl Mock {
    fn new(value: Value) -> Self {
        Self {
            value,
            calls: Cell::new(0),
            error: None,
            dropped: Cell::new(false),
            pending: false,
        }
    }
}
struct Guard<'a>(&'a Cell<bool>);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
impl AccountRpc for Mock {
    async fn account(&self, _: &Pubkey) -> Result<Value, AccountError> {
        self.calls.set(self.calls.get() + 1);
        if self.pending {
            let _guard = Guard(&self.dropped);
            return std::future::pending().await;
        }
        if self.error.is_some() {
            return Err(AccountError::Rpc { code: -32602 });
        }
        Ok(self.value.clone())
    }
}
#[tokio::test]
async fn metadata_and_bounded_data_are_available_without_serializing_payload() {
    let mock = Mock::new(response(b"private-looking-fixture"));
    let observation = account_readonly::query(&mock, &address(), TIME)
        .await
        .unwrap();
    assert_eq!(observation.data(), b"private-looking-fixture");
    let report = serde_json::to_value(&observation.report).unwrap();
    assert_eq!(
        report["account"]["owner"],
        "11111111111111111111111111111111"
    );
    assert_eq!(report["account"]["executable"], false);
    assert_eq!(report["account"]["balance_kelvin"], 42);
    assert_eq!(report["account"]["rent_epoch"], 7);
    assert_eq!(report["account"]["space"], 9999);
    assert_eq!(report["context"]["slot"], 123);
    assert_eq!(report["account"]["data"]["returned_bytes"], 23);
    let text = report.to_string();
    assert!(!text.contains("private-looking-fixture"));
    assert!(!text.contains(&STANDARD.encode(b"private-looking-fixture")));
    assert_eq!(mock.calls.get(), 1);
}
#[tokio::test]
async fn explicit_null_is_not_found_without_retry() {
    let mock = Mock::new(json!({"context":{"slot":1},"value":null}));
    let result = account_readonly::query(&mock, &address(), TIME)
        .await
        .unwrap();
    assert_eq!(result.report.status, "not_found");
    assert!(result.report.account.is_none());
    assert!(result.data().is_empty());
    assert_eq!(mock.calls.get(), 1);
}
#[tokio::test]
async fn missing_or_malformed_required_fields_are_not_absence() {
    let mut fixtures = vec![
        json!({"context":{"slot":1}}),
        json!({"value":null}),
        Value::Null,
        json!([]),
        json!({"error":{"code":-1}}),
    ];
    for field in [
        "owner",
        "executable",
        "kelvin",
        "rentEpoch",
        "space",
        "data",
    ] {
        let mut v = response(b"");
        v["value"].as_object_mut().unwrap().remove(field);
        fixtures.push(v);
    }
    for value in fixtures {
        let mock = Mock::new(value);
        assert!(matches!(
            account_readonly::query(&mock, &address(), TIME).await,
            Err(AccountError::MalformedResponse)
        ));
    }
}
#[tokio::test]
async fn data_cap_is_enforced_before_and_after_decode() {
    let mock = Mock::new(response(&vec![0; MAX_DATA_BYTES]));
    assert_eq!(
        account_readonly::query(&mock, &address(), TIME)
            .await
            .unwrap()
            .data()
            .len(),
        MAX_DATA_BYTES
    );
    for n in [MAX_DATA_BYTES + 1, MAX_DATA_BYTES * 2] {
        let mock = Mock::new(response(&vec![0; n]));
        assert!(matches!(
            account_readonly::query(&mock, &address(), TIME).await,
            Err(AccountError::DataLimit)
        ));
        assert_eq!(mock.calls.get(), 1);
    }
}
#[tokio::test]
async fn unknown_encoding_is_rejected_without_guessing() {
    let mut v = response(b"data");
    v["value"]["data"][1] = json!("base58");
    assert!(matches!(
        account_readonly::query(&Mock::new(v), &address(), TIME).await,
        Err(AccountError::UnsupportedEncoding)
    ));
}
#[tokio::test]
async fn malformed_base64_and_data_container_are_rejected() {
    for data in [
        json!(["%%%", "base64"]),
        json!(["", "base64", "extra"]),
        json!([]),
        json!(["base64"]),
        json!("hidden"),
    ] {
        let mut v = response(b"");
        v["value"]["data"] = data;
        assert!(matches!(
            account_readonly::query(&Mock::new(v), &address(), TIME).await,
            Err(AccountError::MalformedResponse)
        ));
    }
}
#[tokio::test]
async fn invalid_owner_cannot_be_reported_as_verified_metadata() {
    let mut v = response(b"");
    v["value"]["owner"] = json!("bad-owner");
    assert!(matches!(
        account_readonly::query(&Mock::new(v), &address(), TIME).await,
        Err(AccountError::MalformedResponse)
    ));
}
#[tokio::test]
async fn invalid_input_never_calls_rpc() {
    for (addr, timeout) in [
        ("bad-address".to_string(), TIME),
        (address(), Duration::ZERO),
        (address(), Duration::from_secs(31)),
    ] {
        let mock = Mock::new(response(b""));
        assert!(matches!(
            account_readonly::query(&mock, &addr, timeout).await,
            Err(AccountError::InvalidInput)
        ));
        assert_eq!(mock.calls.get(), 0);
    }
}
#[tokio::test]
async fn rpc_error_is_classified_and_never_retried() {
    let mut mock = Mock::new(response(b""));
    mock.error = Some(AccountError::Rpc { code: -32602 });
    assert!(matches!(
        account_readonly::query(&mock, &address(), TIME).await,
        Err(AccountError::Rpc { code: -32602 })
    ));
    assert_eq!(mock.calls.get(), 1);
}
#[test]
fn sdk_errors_keep_codes_but_discard_messages_and_bodies() {
    for (status, expected) in [
        (None, json!({"kind":"rpc","code":-32000})),
        (Some(429), json!({"kind":"http","status":429,"code":-32000})),
    ] {
        let error = AccountError::from(RialoError::Rpc(RpcErrorDetails::new(
            status,
            -32000,
            "sensitive-provider-message".into(),
        )));
        assert_eq!(serde_json::to_value(&error).unwrap(), expected);
        assert!(!format!("{error:?}").contains("sensitive"));
    }
    assert_eq!(
        AccountError::from(RialoError::Io(std::io::Error::other("sensitive"))),
        AccountError::Transport
    );
    assert_eq!(
        AccountError::from(RialoError::Json(
            serde_json::from_str::<Value>("bad-json").unwrap_err()
        )),
        AccountError::MalformedResponse
    );
}
#[tokio::test]
async fn timeout_cancels_pending_request_without_retry() {
    let mut mock = Mock::new(response(b""));
    mock.pending = true;
    assert!(matches!(
        account_readonly::query(&mock, &address(), TIME).await,
        Err(AccountError::Timeout)
    ));
    assert!(mock.dropped.get());
    assert_eq!(mock.calls.get(), 1);
}
#[test]
fn cli_defaults_to_validation_and_rejects_unsafe_or_duplicate_options() {
    let args = vec!["--address".into(), address()];
    let options = account_readonly::parse(&args).unwrap();
    assert!(!options.execute);
    assert_eq!(options.timeout, Duration::from_secs(10));
    for tail in [
        vec!["--address", "x"],
        vec!["--timeout-secs"],
        vec!["--timeout-secs", "0"],
        vec!["--timeout-secs", "1", "--timeout-secs", "2"],
        vec!["--rpc", "https://example.com"],
        vec!["--send"],
        vec!["--execute", "--execute"],
    ] {
        let mut bad = args.clone();
        bad.extend(tail.into_iter().map(str::to_string));
        assert!(account_readonly::parse(&bad).is_err());
    }
    let mut live = args;
    live.push("--execute".into());
    assert!(account_readonly::parse(&live).unwrap().execute);
}
