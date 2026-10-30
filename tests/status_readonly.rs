//! Synthetic fixtures only. No keys, signing, HTTP client, or external calls.
use rialo_cdk::rpc::types::{GetSignatureStatusesRequest, Signature};
use rialo_first::{
    Result,
    monitor::Execution,
    status_readonly::{self, StatusRpc},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    time::Duration,
};
const TIME: Duration = Duration::from_secs(1);
fn ids(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let mut bytes = [0; 64];
            bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
            Signature::from(bytes).to_string()
        })
        .collect()
}
struct Mock {
    value: Value,
    calls: Cell<usize>,
    requests: RefCell<Vec<Value>>,
    mode: u8,
    dropped: Cell<bool>,
}
impl Mock {
    fn new(value: Value) -> Self {
        Self {
            value,
            calls: Cell::new(0),
            requests: RefCell::new(vec![]),
            mode: 0,
            dropped: Cell::new(false),
        }
    }
}
struct Guard<'a>(&'a Cell<bool>);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
impl StatusRpc for Mock {
    async fn signature_statuses(&self, request: GetSignatureStatusesRequest) -> Result<Value> {
        self.calls.set(self.calls.get() + 1);
        self.requests
            .borrow_mut()
            .push(serde_json::to_value(request)?);
        match self.mode {
            1 => Err("synthetic RPC failure".into()),
            2 => {
                let _guard = Guard(&self.dropped);
                std::future::pending().await
            }
            _ => Ok(self.value.clone()),
        }
    }
}
fn response() -> Value {
    json!({"version":0,"context":{"slot":999,"api_version":"synthetic"},"value":[{"slot":12,"executed":true,"err":null},{"slot":13,"executed":false,"err":null},{"slot":14,"executed":true,"err":"AccountNotFound"}]})
}

#[tokio::test]
async fn wire_slots_typed_errors_and_input_order_are_preserved() {
    let mock = Mock::new(response());
    let signatures = ids(3);
    let report = status_readonly::query(&mock, &signatures, TIME)
        .await
        .unwrap();
    assert_eq!(report.response.context.slot, 999);
    assert_eq!(
        report.response.context.api_version.as_deref(),
        Some("synthetic")
    );
    assert_eq!(
        report
            .response
            .value
            .iter()
            .map(|s| s.slot)
            .collect::<Vec<_>>(),
        [12, 13, 14]
    );
    assert_eq!(
        report
            .transactions
            .iter()
            .map(|s| s.signature.clone())
            .collect::<Vec<_>>(),
        signatures
    );
    assert_eq!(report.transactions[0].execution, Execution::Succeeded);
    assert_eq!(report.transactions[1].execution, Execution::Unknown);
    assert_eq!(report.transactions[2].execution, Execution::Failed);
    assert_eq!(serde_json::to_value(&report.response).unwrap(), response());
    assert_eq!(mock.calls.get(), 1);
    assert_eq!(
        mock.requests.borrow()[0],
        json!({"version":0,"signatures":signatures})
    );
}
#[tokio::test]
async fn malformed_or_null_results_never_imply_success_or_absence() {
    let original = response();
    let mut values = vec![Value::Null, json!([]), json!({"error":{"code":-1}})];
    for path in ["context", "value"] {
        let mut v = original.clone();
        v.as_object_mut().unwrap().remove(path);
        values.push(v);
    }
    for field in ["slot", "executed"] {
        let mut v = original.clone();
        v["value"][0].as_object_mut().unwrap().remove(field);
        values.push(v);
    }
    let mut v = original.clone();
    v["value"][0] = Value::Null;
    values.push(v);
    let mut v = original.clone();
    v["value"][0]["err"] = json!("UnknownErrorVariant");
    values.push(v);
    for value in values {
        let mock = Mock::new(value);
        assert!(status_readonly::query(&mock, &ids(3), TIME).await.is_err());
        assert_eq!(mock.calls.get(), 1);
    }
}
#[tokio::test]
async fn count_or_version_mismatch_is_rejected() {
    for value in [
        json!({"context":{"slot":1},"value":[]}),
        json!({"version":1,"context":{"slot":1},"value":[]}),
        response(),
    ] {
        let mock = Mock::new(value);
        assert!(status_readonly::query(&mock, &ids(1), TIME).await.is_err());
        assert_eq!(mock.calls.get(), 1);
    }
    let mut v = response();
    v["version"] = json!(1);
    assert!(
        status_readonly::query(&Mock::new(v), &ids(3), TIME)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn invalid_inputs_make_zero_requests() {
    let mut duplicate = ids(1);
    duplicate.push(duplicate[0].clone());
    for signatures in [vec![], vec!["invalid".into()], ids(257), duplicate] {
        let mock = Mock::new(response());
        assert!(
            status_readonly::query(&mock, &signatures, TIME)
                .await
                .is_err()
        );
        assert_eq!(mock.calls.get(), 0);
    }
    for timeout in [Duration::ZERO, Duration::from_secs(31)] {
        let mock = Mock::new(response());
        assert!(
            status_readonly::query(&mock, &ids(1), timeout)
                .await
                .is_err()
        );
        assert_eq!(mock.calls.get(), 0);
    }
}
#[tokio::test]
async fn maximum_batch_uses_one_request() {
    let mock = Mock::new(
        json!({"context":{"slot":10},"value":(0..256).map(|_|json!({"slot":1,"executed":false})).collect::<Vec<_>>()}),
    );
    let report = status_readonly::query(&mock, &ids(256), TIME)
        .await
        .unwrap();
    assert_eq!(report.transactions.len(), 256);
    assert_eq!(mock.calls.get(), 1);
}
#[tokio::test]
async fn rpc_failure_is_not_retried() {
    let mut mock = Mock::new(response());
    mock.mode = 1;
    assert!(status_readonly::query(&mock, &ids(3), TIME).await.is_err());
    assert_eq!(mock.calls.get(), 1);
}
#[tokio::test]
async fn timeout_cancels_request_without_retry() {
    let mut mock = Mock::new(response());
    mock.mode = 2;
    assert!(status_readonly::query(&mock, &ids(3), TIME).await.is_err());
    assert_eq!(mock.calls.get(), 1);
    assert!(mock.dropped.get());
}
#[test]
fn cli_defaults_to_validation_and_has_no_endpoint_or_write_options() {
    let mut args = vec!["--signature".into(), ids(1)[0].clone()];
    let options = status_readonly::parse(&args).unwrap();
    assert!(!options.execute);
    assert_eq!(options.timeout, Duration::from_secs(10));
    args.push("--execute".into());
    assert!(status_readonly::parse(&args).unwrap().execute);
    for option in ["--rpc", "--send", "--sign", "--execute"] {
        let mut bad = args.clone();
        bad.push(option.into());
        assert!(status_readonly::parse(&bad).is_err());
    }
}
#[test]
fn cli_rejects_missing_values_unknown_flags_and_repeated_timeout() {
    let good = vec!["--signature".into(), ids(1)[0].clone()];
    for tail in [
        vec!["--signature"],
        vec!["--timeout-secs"],
        vec!["--timeout-secs", "0"],
        vec!["--timeout-secs", "NaN"],
        vec!["--timeout-secs", "1", "--timeout-secs", "2"],
        vec!["--unknown"],
    ] {
        let mut args = good.clone();
        args.extend(tail.into_iter().map(str::to_string));
        assert!(status_readonly::parse(&args).is_err());
    }
}
