//! Offline replay only. Build with existing compiled dependencies; never constructs an HTTP client.
use rialo_cdk::rpc::types::{GetSignaturesForAddressConfig, Signature, SignatureStatus};
use rialo_first::{
    Result,
    monitor::{
        self,
        query::{self, Query, QueryRpc},
    },
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, str::FromStr, time::Duration};
const T: Duration = Duration::from_secs(2);
struct Replay {
    replies: RefCell<VecDeque<Value>>,
    requests: RefCell<Vec<Value>>,
}
impl Replay {
    fn new(replies: Vec<Value>) -> Self {
        Self {
            replies: RefCell::new(replies.into()),
            requests: RefCell::new(vec![]),
        }
    }
    fn exhausted(&self) {
        assert!(
            self.replies.borrow().is_empty(),
            "unconsumed saved responses"
        );
    }
}
impl QueryRpc for Replay {
    async fn query(&self, q: Query) -> Result<Value> {
        self.requests.borrow_mut().push(match q {
            Query::History(r) => json!([r]),
            Query::Detail(s) => json!([{"version":0,"signature":s.to_string()}]),
        });
        self.replies
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| "unexpected replay request".into())
    }
}
fn config() -> GetSignaturesForAddressConfig {
    GetSignaturesForAddressConfig {
        limit: Some(2),
        before: None,
        until: None,
    }
}
async fn verify(saved: Value) -> Result<Value> {
    let mut checks = vec![];
    let detail_calls: Vec<_> = saved["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == "getTransaction")
        .collect();
    assert_eq!(detail_calls.len(), 5);
    for (call, expected) in detail_calls
        .iter()
        .zip(saved["details"].as_array().unwrap())
    {
        let replay = Replay::new(vec![call["result"].clone()]);
        let detail =
            query::transaction_detail(&replay, expected["signature"].as_str().unwrap(), T).await?;
        assert_eq!(serde_json::to_value(&detail)?, expected["detail"]);
        assert_eq!(replay.requests.borrow()[0], call["params"]);
        replay.exhausted();
        // Response data conversion only; no TransactionBuilder, wallet or signature calculation.
        let _: rialo_cdk::generated::types::TransactionData = detail.transaction.try_into()?;
        let _: rialo_cdk::generated::types::TransactionStatusMetadata = detail.meta.into();
    }
    checks.push(json!({"name":"saved_detail_and_sdk_conversion_5", "status":"PASS", "evidence":"recorded_live_response_replay"}));
    let statuses: Vec<Option<SignatureStatus>> =
        serde_json::from_value(saved["status"]["sdk_statuses"].clone())?;
    assert_eq!(statuses.len(), 5);
    for ((s, candidate), expected) in statuses
        .into_iter()
        .zip(saved["candidates"].as_array().unwrap())
        .zip(saved["status"]["monitor_observations"].as_array().unwrap())
    {
        assert!(s.as_ref().unwrap().executed && s.as_ref().unwrap().err.is_none());
        let id = Signature::from_str(candidate["signature"].as_str().unwrap())?;
        assert_eq!(
            serde_json::to_value(monitor::observe_status(&id, s))?,
            *expected
        );
    }
    checks.push(json!({"name":"saved_sdk_status_and_reason_5", "status":"PASS", "evidence":"SDK_converted_status_not_raw_wire"}));
    let calls: Vec<_> = saved["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == "getSignaturesForAddress")
        .collect();
    assert_eq!(calls.len(), 2);
    let p1 = calls[0]["result"].clone();
    let p2 = calls[1]["result"].clone();
    let address = saved["histories"][0]["address"].as_str().unwrap();
    let replay = Replay::new(vec![p1.clone(), p2.clone()]);
    let batch = query::history_pages(&replay, address, config(), 2, T).await?;
    assert_eq!(batch.entries.len(), 4);
    assert_eq!(batch.pages_read, 2);
    assert_eq!(batch.duplicates_removed, 0);
    assert_eq!(
        batch.next_before,
        saved["histories"][1]["page"]["next_before"]
            .as_str()
            .map(String::from)
    );
    for (actual, expected) in replay.requests.borrow().iter().zip(&calls) {
        assert_eq!(*actual, expected["params"]);
    }
    replay.exhausted();
    checks.push(json!({"name":"saved_two_page_batch_cursor_and_budget", "status":"PASS", "evidence":"recorded_live_response_replay"}));
    for p in [&p1, &p2] {
        assert_eq!(p["context"]["api_version"], "0.23.0-alpha.0");
        let decoded: rialo_cdk::rpc::types::GetSignaturesForAddressResponse =
            serde_json::from_value(p.clone())?;
        assert_eq!(decoded.value.len(), 2);
    }
    checks.push(json!({"name":"saved_context_value_and_api_version", "status":"PASS", "evidence":"recorded_live_response_replay"}));
    let empty = json!({"context":p2["context"],"value":[]});
    let replay = Replay::new(vec![p1.clone(), p2.clone(), empty.clone()]);
    let batch = query::history_pages(&replay, address, config(), 5, T).await?;
    assert_eq!(batch.entries.len(), 4);
    assert_eq!(batch.pages_read, 3);
    assert!(batch.next_before.is_none());
    replay.exhausted();
    checks.push(json!({"name":"synthetic_empty_terminal_after_saved_pages", "status":"PASS", "evidence":"synthetic_terminal_not_live"}));
    let mut short = p2.clone();
    short["value"].as_array_mut().unwrap().truncate(1);
    let replay = Replay::new(vec![p1.clone(), short]);
    let batch = query::history_pages(&replay, address, config(), 5, T).await?;
    assert_eq!(batch.entries.len(), 3);
    assert_eq!(batch.pages_read, 2);
    assert!(batch.next_before.is_none());
    replay.exhausted();
    checks.push(json!({"name":"synthetic_short_terminal", "status":"PASS", "evidence":"synthetic_derived_page_not_live"}));
    let mut overlap = p2.clone();
    overlap["value"] = json!([p1["value"][1], p2["value"][0]]);
    let replay = Replay::new(vec![p1.clone(), overlap.clone(), empty]);
    let batch = query::history_pages(&replay, address, config(), 3, T).await?;
    assert_eq!(batch.entries.len(), 3);
    assert_eq!(batch.duplicates_removed, 1);
    assert!(batch.next_before.is_none());
    replay.exhausted();
    checks.push(json!({"name":"synthetic_boundary_overlap_preserves_distinct_entries", "status":"PASS", "evidence":"synthetic_overlap_not_live"}));
    overlap["value"][0]["blockHeight"] = json!(1);
    assert!(
        query::history_pages(
            &Replay::new(vec![p1.clone(), overlap]),
            address,
            config(),
            3,
            T
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("conflicting duplicate")
    );
    checks.push(json!({"name":"synthetic_conflicting_boundary_duplicate_rejected", "status":"PASS", "evidence":"synthetic_conflict_not_live"}));
    assert!(
        query::history_pages(
            &Replay::new(vec![p1.clone(), p2, p1.clone()]),
            address,
            config(),
            3,
            T
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("cycle")
    );
    checks.push(json!({"name":"synthetic_cursor_cycle_rejected", "status":"PASS", "evidence":"synthetic_cycle_not_live"}));
    assert!(
        serde_json::from_value::<Vec<rialo_cdk::generated::types::SignatureInfo>>(p1.clone())
            .is_err()
    );
    let unwrapped: Vec<rialo_cdk::generated::types::SignatureInfo> =
        serde_json::from_value(p1["value"].clone())?;
    assert_eq!(unwrapped.len(), 2);
    checks.push(json!({"name":"SDK_history_convenience_outer_shape_mismatch_reproduced", "status":"PASS", "compatibility":"FAIL_for_unmodified_SDK_convenience_decoder_only", "evidence":"saved_wrapper_fails_Vec_inner_value_decodes_Monitor_works"}));
    let old = [
        "5TiohMeL3ryJJPQKqJ7Dxr7oUAj2ZNnu9qRv1m3Sws6omMQDT6x9vvUtNbthVubd1Nvdeb6dkGdMJ6uGPHC8myz7",
        "5o9vrT8LrU3U6Gith4FxCRzHTrHNCm5xJ9AWQCQB8A9yxaXgu1oWP4xdVJhmPD2P8oRB4oouY1StAjSnWkQKmFG8",
        "5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs",
    ];
    for id in old {
        assert_eq!(Signature::from_str(id)?.to_string(), id);
    }
    checks.push(json!({"name":"historical_three_signature_syntax", "status":"PASS", "evidence":"syntax_only_not_chain_presence_or_crypto_verification"}));
    Ok(
        json!({"status":"passed","network_requests":0,"checks":checks,"passed":11,"failed":0,"live_terminal":"UNKNOWN","retention":"UNKNOWN","old_lookup_cause":"UNKNOWN","raw_status_wire":"UNKNOWN"}),
    )
}
fn main() -> Result<()> {
    let path = std::env::args().nth(1).ok_or("saved JSON path required")?;
    let saved: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let result = tokio::runtime::Runtime::new()?.block_on(verify(saved))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// Additional, manually audited tests for gaps in the original 71-test allowlist.
// Kept outside Cargo's existing targets so its audited inventory stays unchanged.
#[cfg(test)]
mod quality_tests {
    use super::*;
    use std::cell::Cell;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/phase7z/synthetic.json")).unwrap()
    }

    #[test]
    fn malformed_or_required_missing_later_page_never_returns_partial_success() {
        let f = fixture();
        let address = f["address"].as_str().unwrap();
        let page = f["page2"].clone();
        let mut missing_signature = page.clone();
        missing_signature["value"][0]
            .as_object_mut()
            .unwrap()
            .remove("signature");
        let mut missing_height = page.clone();
        missing_height["value"][0]
            .as_object_mut()
            .unwrap()
            .remove("blockHeight");
        let mut invalid_signature = page.clone();
        invalid_signature["value"][0]["signature"] = json!("invalid");
        let mut null_height = page.clone();
        null_height["value"][0]["blockHeight"] = Value::Null;
        let cases = [
            ("bare array", json!([])),
            ("null", Value::Null),
            ("missing value", json!({"context":{"slot":150}})),
            ("missing context", json!({"value":[]})),
            ("missing slot", json!({"context":{},"value":[]})),
            ("null value", json!({"context":{"slot":150},"value":null})),
            (
                "wrong slot type",
                json!({"context":{"slot":"150"},"value":[]}),
            ),
            ("missing signature", missing_signature),
            ("missing height", missing_height),
            ("invalid response signature", invalid_signature),
            ("null height", null_height),
            ("excess count", f["history"].clone()),
        ];
        let runtime = tokio::runtime::Runtime::new().unwrap();
        for (label, bad) in cases {
            let replay = Replay::new(vec![f["page1"].clone(), bad]);
            let result = runtime.block_on(query::history_pages(&replay, address, config(), 5, T));
            assert!(
                result.is_err(),
                "{label} must not produce an apparently successful partial batch"
            );
            assert_eq!(
                replay.requests.borrow().len(),
                2,
                "{label}: must stop without retries"
            );
            replay.exhausted();
        }
    }

    #[test]
    fn absent_optional_fields_on_later_page_remain_valid() {
        let f = fixture();
        let mut short = f["short_page"].clone();
        let entry = short["value"][0].as_object_mut().unwrap();
        entry.remove("err");
        entry.remove("blockTime");
        let replay = Replay::new(vec![f["page1"].clone(), short]);
        let batch = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(query::history_pages(
                &replay,
                f["address"].as_str().unwrap(),
                config(),
                5,
                T,
            ))
            .unwrap();
        assert_eq!(batch.entries.len(), 3);
        assert!(batch.entries[2].err.is_none());
        assert!(batch.entries[2].block_time.is_none());
        assert!(batch.next_before.is_none());
        assert_eq!(batch.pages_read, 2);
        replay.exhausted();
    }

    struct FaultAfterFirst {
        page: Value,
        calls: Cell<usize>,
        timeout: bool,
        cancelled: Cell<bool>,
    }
    struct Cancelled<'a>(&'a Cell<bool>);
    impl Drop for Cancelled<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    impl QueryRpc for FaultAfterFirst {
        async fn query(&self, request: Query) -> Result<Value> {
            let Query::History(request) = request else {
                panic!("unexpected detail request")
            };
            let n = self.calls.get();
            self.calls.set(n + 1);
            match n {
                0 => Ok(self.page.clone()),
                1 => {
                    assert_eq!(
                        request.config.unwrap().before.as_deref(),
                        self.page["value"][1]["signature"].as_str()
                    );
                    if self.timeout {
                        let _cancelled = Cancelled(&self.cancelled);
                        std::future::pending().await
                    } else {
                        Err("synthetic second-page RPC error".into())
                    }
                }
                _ => panic!("must stop after second-page failure"),
            }
        }
    }

    fn check_later_failure(timeout: bool) {
        let f = fixture();
        let rpc = FaultAfterFirst {
            page: f["page1"].clone(),
            calls: Cell::new(0),
            timeout,
            cancelled: Cell::new(false),
        };
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(query::history_pages(
                &rpc,
                f["address"].as_str().unwrap(),
                config(),
                5,
                Duration::from_millis(100),
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains(if timeout {
            "history RPC timeout"
        } else {
            "synthetic second-page RPC error"
        }));
        assert_eq!(rpc.calls.get(), 2);
        assert_eq!(rpc.cancelled.get(), timeout);
    }

    #[test]
    fn second_page_rpc_error_stops_without_partial_success_or_retry() {
        check_later_failure(false);
    }

    #[test]
    fn second_page_timeout_cancels_pending_request_without_partial_success() {
        check_later_failure(true);
    }
}
