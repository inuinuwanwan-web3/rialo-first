//! Offline only: response fixtures, no transaction building or signing.
use rialo_cdk::rpc::types::{
    GetSignaturesForAddressConfig, GetTransactionResponse, Pubkey, Signature, SignatureStatus,
};
use rialo_first::{
    Result,
    monitor::{
        self, Execution, MonitorRpc, StatusReason,
        cli::{self, ConfigRpc},
        query::{self, Query, QueryRpc},
    },
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, str::FromStr, time::Duration};
const TIME: Duration = Duration::from_millis(100);
fn f() -> Value {
    serde_json::from_str(include_str!("fixtures/phase7z/synthetic.json")).unwrap()
}
struct Mock {
    replies: RefCell<VecDeque<Value>>,
    seen: RefCell<Vec<Option<String>>>,
    status: Option<SignatureStatus>,
    mode: u8,
}
fn mock(values: Vec<Value>) -> Mock {
    Mock {
        replies: RefCell::new(values.into()),
        seen: RefCell::new(vec![]),
        status: Some(serde_json::from_value(f()["success"].clone()).unwrap()),
        mode: 0,
    }
}
impl QueryRpc for Mock {
    async fn query(&self, q: Query) -> Result<Value> {
        self.seen.borrow_mut().push(match q {
            Query::History(r) => r.config.unwrap().before,
            Query::Detail(_) => None,
        });
        match self.mode {
            1 => Err("synthetic RPC error".into()),
            2 => std::future::pending().await,
            _ => Ok(self
                .replies
                .borrow_mut()
                .pop_front()
                .expect("unexpected query")),
        }
    }
}
impl MonitorRpc for Mock {
    async fn balance(&self, _: &Pubkey) -> Result<u64> {
        Ok(42)
    }
    async fn height(&self) -> Result<u64> {
        Ok(150)
    }
    async fn statuses(&self, ids: &[Signature]) -> Result<Vec<Option<SignatureStatus>>> {
        match self.mode {
            1 => Err("synthetic status error".into()),
            2 => std::future::pending().await,
            3 => Ok(vec![]),
            _ => Ok(ids.iter().map(|_| self.status.clone()).collect()),
        }
    }
}
impl ConfigRpc for Mock {
    async fn config_prefix(&self) -> Result<u64> {
        Ok(42)
    }
}
fn config() -> GetSignaturesForAddressConfig {
    GetSignaturesForAddressConfig {
        limit: Some(2),
        before: None,
        until: None,
    }
}
fn address() -> String {
    f()["address"].as_str().unwrap().into()
}
fn id() -> String {
    f()["signatures"][0].as_str().unwrap().into()
}
#[test]
fn synthetic_sdk_shapes_and_status_reasons() {
    let v = f();
    assert_eq!(v["synthetic"], true);
    for (key, execution, reason) in [
        (
            "success",
            Execution::Succeeded,
            StatusReason::ExecutedWithoutError,
        ),
        ("failure", Execution::Failed, StatusReason::RpcReportedError),
        (
            "unexecuted",
            Execution::Unknown,
            StatusReason::UnexecutedOrNotFound,
        ),
    ] {
        let status = serde_json::from_value(v[key].clone()).unwrap();
        let o = monitor::observe_status(&Signature::from_str(&id()).unwrap(), Some(status));
        assert_eq!(o.execution, execution);
        assert_eq!(o.status_reason, reason);
    }
    let o = monitor::observe_status(&Signature::from_str(&id()).unwrap(), None);
    assert_eq!(o.execution, Execution::Unknown);
    assert_eq!(o.status_reason, StatusReason::NoStatusReturned);
    assert!(serde_json::from_value::<SignatureStatus>(v["incomplete_status"].clone()).is_err());
}
#[tokio::test]
async fn details_preserve_error_and_height() {
    for key in ["detail_success", "detail_failure"] {
        let v = f();
        let m = mock(vec![v[key].clone()]);
        let d = query::transaction_detail(&m, &id(), TIME).await.unwrap();
        assert_eq!(d.block_height, 120);
        assert_eq!(d.transaction.signatures[0], id());
        assert_eq!(
            serde_json::to_value(d).unwrap()["meta"]["err"],
            v[key]["meta"]["err"]
        );
    }
    let d: GetTransactionResponse = serde_json::from_value(f()["detail_success"].clone()).unwrap();
    assert!(d.meta.err.is_none());
}
#[tokio::test]
async fn missing_and_incomplete_details_do_not_imply_success() {
    for key in ["not_found", "incomplete_detail"] {
        let m = mock(vec![f()[key].clone()]);
        assert!(query::transaction_detail(&m, &id(), TIME).await.is_err());
        let mut m = mock(vec![]);
        m.status = None;
        assert_eq!(
            monitor::snapshot(&m, &address(), &[id()], TIME)
                .await
                .unwrap()
                .transactions[0]
                .execution,
            Execution::Unknown
        );
    }
}
#[tokio::test]
async fn nonempty_history_preserves_provider_order_and_fields() {
    let v = f();
    let m = mock(vec![v["history"].clone()]);
    let mut c = config();
    c.limit = Some(20);
    let p = query::history_page(&m, &address(), c, TIME).await.unwrap();
    assert_eq!(p.entries.len(), 3);
    for (e, expected) in p
        .entries
        .iter()
        .zip(v["history"]["value"].as_array().unwrap())
    {
        assert_eq!(e.signature, expected["signature"]);
        assert_eq!(e.block_height, expected["blockHeight"]);
    }
    assert!(p.next_before.is_none());
}
#[tokio::test]
async fn pagination_deduplicates_boundary_and_finishes() {
    let v = f();
    let m = mock(vec![
        v["page1"].clone(),
        v["page2"].clone(),
        v["terminal"].clone(),
    ]);
    let b = query::history_pages(&m, &address(), config(), 3, TIME)
        .await
        .unwrap();
    assert_eq!(b.entries.len(), 3);
    assert_eq!(b.duplicates_removed, 1);
    assert_eq!(b.pages_read, 3);
    assert!(b.next_before.is_none());
    assert_eq!(
        *m.seen.borrow(),
        vec![
            None,
            Some(v["signatures"][1].as_str().unwrap().into()),
            Some(v["signatures"][2].as_str().unwrap().into())
        ]
    );
    let j = serde_json::to_value(b).unwrap();
    assert_eq!(j["entries"][2]["blockHeight"], 130);
}
#[tokio::test]
async fn budget_retains_cursor_and_short_page_terminates() {
    let v = f();
    let m = mock(vec![v["page1"].clone()]);
    let b = query::history_pages(&m, &address(), config(), 1, TIME)
        .await
        .unwrap();
    assert_eq!(b.next_before.as_deref(), v["signatures"][1].as_str());
    let m = mock(vec![v["page1"].clone(), v["short_page"].clone()]);
    let b = query::history_pages(&m, &address(), config(), 5, TIME)
        .await
        .unwrap();
    assert_eq!(b.pages_read, 2);
    assert!(b.next_before.is_none());
}
#[tokio::test]
async fn conflicts_and_cursor_cycles_are_errors() {
    let v = f();
    let mut bad = v["page2"].clone();
    bad["value"][0]["blockHeight"] = json!(999);
    assert!(
        query::history_pages(
            &mock(vec![v["page1"].clone(), bad]),
            &address(),
            config(),
            3,
            TIME
        )
        .await
        .is_err()
    );
    assert!(
        query::history_pages(
            &mock(vec![
                v["page1"].clone(),
                v["page2"].clone(),
                v["page1"].clone()
            ]),
            &address(),
            config(),
            3,
            TIME
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("cycle")
    );
}
#[tokio::test]
async fn typed_history_transaction_error_is_preserved() {
    let m = mock(vec![f()["history_failure"].clone()]);
    let p = query::history_page(&m, &address(), config(), TIME)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&p.entries[0]).unwrap()["err"],
        "AccountNotFound"
    );
    assert!(p.entries[0].err.is_some());
}
#[tokio::test]
async fn invalid_inputs_do_not_call_rpc() {
    let m = mock(vec![]);
    assert!(
        query::transaction_detail(&m, "invalid", TIME)
            .await
            .is_err()
    );
    for until in [false, true] {
        let mut c = config();
        if until {
            c.until = Some("invalid".into())
        } else {
            c.before = Some("invalid".into())
        }
        assert!(
            query::history_pages(&m, &address(), c, 2, TIME)
                .await
                .is_err()
        );
    }
    for budget in [0, 101] {
        assert!(
            query::history_pages(&m, &address(), config(), budget, TIME)
                .await
                .is_err()
        );
    }
    assert!(m.seen.borrow().is_empty());
}
#[tokio::test]
async fn rpc_errors_timeouts_and_empty_history() {
    for mode in [1, 2] {
        let mut m = mock(vec![]);
        m.mode = mode;
        assert!(query::transaction_detail(&m, &id(), TIME).await.is_err());
        assert!(
            query::history_pages(&m, &address(), config(), 3, TIME)
                .await
                .is_err()
        );
        assert!(
            monitor::snapshot(&m, &address(), &[id()], TIME)
                .await
                .is_err()
        );
    }
    let m = mock(vec![f()["terminal"].clone()]);
    let b = query::history_pages(&m, &address(), config(), 3, TIME)
        .await
        .unwrap();
    assert_eq!(b.pages_read, 1);
    assert!(b.entries.is_empty());
    assert!(b.next_before.is_none());
}
#[tokio::test]
async fn cli_json_and_errors_reuse_existing_monitor() {
    let opts =
        cli::Options::parse(&["--address".into(), address(), "--tx-hash".into(), id()]).unwrap();
    for (key, execution, reason) in [
        ("success", "Confirmed", "ExecutedWithoutError"),
        ("failure", "Failed", "RpcReportedError"),
        ("unexecuted", "Unknown", "UnexecutedOrNotFound"),
    ] {
        let mut m = mock(vec![]);
        m.status = Some(serde_json::from_value(f()[key].clone()).unwrap());
        let o = cli::execute(&opts, &m).await;
        assert_eq!(o.code, 0);
        assert!(o.stderr.is_empty());
        let j: Value = serde_json::from_str(&o.stdout).unwrap();
        assert_eq!(j["transactions"][0]["execution"], execution);
        assert_eq!(j["transactions"][0]["status_reason"], reason);
    }
    for mode in [1, 3] {
        let mut m = mock(vec![]);
        m.mode = mode;
        let o = cli::execute(&opts, &m).await;
        assert_eq!(o.code, 1);
        assert_eq!(o.stderr, "monitor RPC or response error");
        assert!(
            serde_json::from_str::<Value>(&o.stdout)
                .unwrap()
                .get("error")
                .is_some()
        );
    }
}
