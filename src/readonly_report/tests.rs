use super::*;
use crate::{
    account_readonly::{DataSummary, PublicAccount},
    monitor::{Execution, StatusReason, TransactionObservation},
};
use std::{cell::Cell, time::Duration};
fn time() -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(1793491200123)
}
fn address() -> String {
    Pubkey::from([1; 32]).to_string()
}
fn signature() -> String {
    Signature::from([1; 64]).to_string()
}
fn account(present: bool) -> AccountReport {
    AccountReport {
        address: address(),
        context: json!({"slot":42,"api_version":"sk-provider-secret","unexpected":"sensitive"}),
        status: if present { "present" } else { "not_found" },
        account: present.then(|| PublicAccount {
            owner: "11111111111111111111111111111111".into(),
            executable: true,
            balance_kelvin: 7,
            rent_epoch: 2,
            space: 4,
            data: DataSummary {
                encoding: "base64",
                returned_bytes: 4,
                requested_offset: 0,
                requested_max_bytes: 4096,
            },
        }),
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        address: address(),
        balance_kelvin: 9,
        block_height: 123,
        transactions: vec![TransactionObservation {
            signature: signature(),
            execution: Execution::Unknown,
            status_reason: StatusReason::UnexecutedOrNotFound,
        }],
        address_history_supported: false,
        confirmation_depth: None,
    }
}
fn parse(report: Result<Report, ReportError>) -> Value {
    let output = finish(report, Network::Devnet, Operation::AccountLookup);
    serde_json::from_str(&output.stdout).unwrap()
}
#[test]
fn schema_network_operation_and_utc_are_machine_readable() {
    let v = parse(Report::account(
        Network::Devnet,
        &account(true),
        None,
        Transport::ProvidedData,
        time(),
    ));
    assert_eq!(v["schema_version"], "1.0");
    assert_eq!(v["network"], "devnet");
    assert_eq!(v["operation"], "account_lookup");
    assert_eq!(v["status"], "success");
    assert_eq!(v["timestamp_utc"], "2026-11-01T00:00:00.123Z");
    assert_eq!(v["verification"], "not_asserted");
    assert_eq!(v["error"], Value::Null);
}
#[test]
fn utc_handles_epoch_leap_centuries_and_rejects_unrepresentable_time() {
    assert_eq!(
        utc_timestamp(UNIX_EPOCH).unwrap(),
        "1970-01-01T00:00:00.000Z"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(1709164800)).unwrap(),
        "2024-02-29T00:00:00.000Z"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(253402300799)).unwrap(),
        "9999-12-31T23:59:59.000Z"
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH - Duration::from_secs(1)),
        Err(ReportError::InvalidTimestamp)
    );
    assert_eq!(
        utc_timestamp(UNIX_EPOCH + Duration::from_secs(253402300800)),
        Err(ReportError::InvalidTimestamp)
    );
}
#[test]
fn not_found_is_a_completed_query_with_null_account_not_transport_failure() {
    let receipt = HttpReceipt {
        http_status: 200,
        body_bytes: 106,
        body_limit: 1048576,
        request_id: 1,
        jsonrpc_version: "2.0",
    };
    let v = parse(Report::account(
        Network::Devnet,
        &account(false),
        Some(&receipt),
        Transport::BoundedHttp,
        time(),
    ));
    assert_eq!(v["status"], "not_found");
    assert_eq!(v["data"]["account"], Value::Null);
    assert_eq!(v["error"], Value::Null);
    assert_eq!(v["context"]["slot"], 42);
    assert_eq!(v["http"]["status"], 200);
    assert_eq!(v["query_attempted"], true);
}
#[test]
fn account_projection_omits_provider_strings_and_raw_data() {
    let v = parse(Report::account(
        Network::Testnet,
        &account(true),
        None,
        Transport::ProvidedData,
        time(),
    ));
    let text = v.to_string();
    assert!(!text.contains("sk-provider"));
    assert!(!text.contains("sensitive"));
    assert!(!text.contains("api_version"));
    assert_eq!(v["data"]["account"]["data"]["returned_bytes"], 4);
    assert_eq!(v["data"]["account"]["executable"], true);
}
#[test]
fn invalid_public_fields_and_inconsistent_absence_fail_closed() {
    let mut a = account(true);
    a.address = "sk-private-looking-input".into();
    assert!(matches!(
        Report::account(Network::Devnet, &a, None, Transport::Sdk, time()),
        Err(ReportError::InvalidData)
    ));
    let mut a = account(true);
    a.status = "not_found";
    assert!(Report::account(Network::Devnet, &a, None, Transport::Sdk, time()).is_err());
    let mut a = account(false);
    a.context = json!({"slot":"secret"});
    assert!(Report::account(Network::Devnet, &a, None, Transport::Sdk, time()).is_err());
}
#[test]
fn all_error_states_preserve_only_codes_and_fixed_reasons() {
    for (error, status) in [
        (AccountError::Rpc { code: -32602 }, "rpc_error"),
        (
            AccountError::Http {
                status: 503,
                code: -1,
            },
            "http_error",
        ),
        (AccountError::Timeout, "timeout"),
        (AccountError::InvalidInput, "blocked"),
    ] {
        let v = parse(Report::failure(
            Network::Devnet,
            Operation::AccountLookup,
            Failure::account(&error),
            Transport::Sdk,
            true,
            time(),
        ));
        assert_eq!(v["status"], status);
        assert_eq!(v["data"], Value::Null);
        assert!(v["error"].is_object());
        if status == "http_error" {
            assert_eq!(v["error"]["http_status"], 503);
        }
        if status == "rpc_error" {
            assert_eq!(v["error"]["code"], -32602);
        }
    }
    let error = RialoError::Rpc(rialo_cdk::error::RpcErrorDetails::new(
        None,
        -32000,
        "sk-secret-provider-body".into(),
    ));
    let v = parse(Report::failure(
        Network::Devnet,
        Operation::AccountLookup,
        Failure::error(&error),
        Transport::Sdk,
        true,
        time(),
    ));
    assert!(!v.to_string().contains("sk-secret"));
    assert_eq!(v["error"]["code"], -32000);
}
#[test]
fn unverified_never_claims_an_executed_query_or_online_pass() {
    let v = parse(Report::unverified(
        Network::Devnet,
        Operation::TransactionStatus,
        Transport::Sdk,
        time(),
    ));
    assert_eq!(v["status"], "unverified");
    assert_eq!(v["query_attempted"], false);
    assert_eq!(v["context"]["slot"], Value::Null);
    assert_eq!(v["data"]["executed"], false);
    assert!(!v.to_string().contains("PASS"));
}
#[test]
fn balance_snapshot_does_not_relabel_height_as_context_slot() {
    let v = parse(Report::monitor(
        Network::Testnet,
        &snapshot(),
        Some(77),
        Transport::ProvidedData,
        time(),
    ));
    assert_eq!(v["context"]["slot"], Value::Null);
    assert_eq!(v["data"]["block_height"], 123);
    assert_eq!(v["data"]["balance_kelvin"], 9);
    assert_eq!(v["data"]["network_config_prefix"], 77);
    assert_eq!(v["query_attempted"], false);
    assert_eq!(v["data"]["transactions"][0]["execution"], "Unknown");
}
#[test]
fn status_adapter_preserves_individual_slots_and_context_separately() {
    let response = serde_json::from_value(
        json!({"context":{"slot":99},"value":[{"slot":12,"executed":false,"err":null}]}),
    )
    .unwrap();
    let input = StatusReport {
        response,
        transactions: snapshot().transactions,
    };
    let v = parse(Report::statuses(
        Network::Devnet,
        &input,
        Transport::ProvidedData,
        time(),
    ));
    assert_eq!(v["status"], "success");
    assert_eq!(v["context"]["slot"], 99);
    assert_eq!(v["data"]["transactions"][0]["slot"], 12);
    assert_eq!(v["query_attempted"], false);
    assert_eq!(v["data"]["transactions"][0]["execution"], "Unknown");
}
#[test]
fn empty_history_and_batch_have_no_false_absence_or_snapshot_claim() {
    let page = HistoryPage {
        context_slot: 8,
        entries: vec![],
        next_before: None,
        duplicates_removed: 0,
    };
    let v = parse(Report::history_page(Network::Devnet, &page, time()));
    assert_eq!(v["status"], "success");
    assert_eq!(v["context"]["slot"], 8);
    assert_eq!(v["data"]["entries"], json!([]));
    let batch = HistoryBatch {
        entries: vec![],
        next_before: None,
        pages_read: 2,
        duplicates_removed: 0,
    };
    let v = parse(Report::history_batch(Network::Testnet, &batch, time()));
    assert_eq!(v["context"]["slot"], Value::Null);
    assert_eq!(v["query_attempted"], false);
    assert_eq!(v["transport"], "provided_data");
}
#[test]
fn entry_limit_is_explicit_and_suppresses_unsafe_resume_cursor() {
    let page = HistoryPage {
        context_slot: 8,
        entries: (0..MAX_ENTRIES + 1)
            .map(|_| SignatureInfo::new(signature(), 7))
            .collect(),
        next_before: Some(signature()),
        duplicates_removed: 0,
    };
    let v = parse(Report::history_page(Network::Testnet, &page, time()));
    assert_eq!(v["data"]["entries"].as_array().unwrap().len(), MAX_ENTRIES);
    assert_eq!(v["data"]["truncated"], true);
    assert_eq!(v["data"]["total_entries"], 257);
    assert_eq!(v["data"]["next_before"], Value::Null);
}
#[test]
fn transaction_detail_drops_message_logs_and_arbitrary_metadata() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/phase7z/synthetic.json")).unwrap();
    let detail: GetTransactionResponse =
        serde_json::from_value(fixture["detail_success"].clone()).unwrap();
    let v = parse(Report::detail(Network::Testnet, &detail, time()));
    assert_eq!(v["operation"], "transaction_detail");
    assert!(v["data"].get("message").is_none());
    assert!(v["data"].get("logs").is_none());
    assert!(v["data"]["signature"].is_string());
}
#[test]
fn writer_exact_boundary_and_one_byte_over_never_emit_partial_json() {
    let report = Report::unverified(
        Network::Devnet,
        Operation::AccountLookup,
        Transport::Sdk,
        time(),
    )
    .unwrap();
    let expected = serde_json::to_vec(&report).unwrap();
    assert_eq!(report.render(expected.len()).unwrap().as_bytes(), expected);
    assert_eq!(
        report.render(expected.len() - 1),
        Err(ReportError::OutputLimit)
    );
    let mut report = report;
    report.data = json!({"test_only":"x".repeat(MAX_JSON_BYTES)});
    let output = finish(Ok(report), Network::Devnet, Operation::AccountLookup);
    assert!(output.failed);
    assert!(output.stdout.len() < MAX_JSON_BYTES);
    let v: Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(v["error"]["reason"], "output_limit");
}
struct Mock {
    calls: Cell<usize>,
    failure: bool,
}
impl monitor::MonitorRpc for Mock {
    async fn balance(&self, _: &Pubkey) -> crate::Result<u64> {
        self.calls.set(self.calls.get() + 1);
        Ok(5)
    }
    async fn height(&self) -> crate::Result<u64> {
        self.calls.set(self.calls.get() + 1);
        Ok(123)
    }
    async fn statuses(&self, ids: &[Signature]) -> crate::Result<Vec<Option<SignatureStatus>>> {
        self.calls.set(self.calls.get() + 1);
        Ok(ids.iter().map(|_| None).collect())
    }
}
impl crate::monitor::cli::ConfigRpc for Mock {
    async fn config_prefix(&self) -> crate::Result<u64> {
        self.calls.set(self.calls.get() + 1);
        if self.failure {
            Err(Box::new(RialoError::Rpc(
                rialo_cdk::error::RpcErrorDetails::new(None, -32602, "sensitive-body".into()),
            )))
        } else {
            Ok(77)
        }
    }
}
#[tokio::test]
async fn unified_monitor_reuses_rpc_count_and_keeps_legacy_output() {
    let args = vec![
        "--address".into(),
        address(),
        "--tx-hash".into(),
        signature(),
    ];
    let mut json_args = args.clone();
    json_args.push("--json".into());
    for (args, unified) in [(args, false), (json_args, true)] {
        let options = crate::monitor::cli::Options::parse(&args).unwrap();
        let mock = Mock {
            calls: Cell::new(0),
            failure: false,
        };
        let output = crate::monitor::cli::execute(&options, &mock).await;
        assert_eq!(output.code, 0);
        assert_eq!(mock.calls.get(), 4);
        let v: Value = serde_json::from_str(&output.stdout).unwrap();
        assert_eq!(v.get("schema_version").is_some(), unified);
        if !unified {
            assert_eq!(v["balance_kelvin"], 5);
        } else {
            assert_eq!(v["data"]["balance_kelvin"], 5);
        }
    }
}
#[tokio::test]
async fn unified_monitor_errors_remain_machine_readable_and_redacted() {
    let options =
        crate::monitor::cli::Options::parse(&["--address".into(), address(), "--json".into()])
            .unwrap();
    let mock = Mock {
        calls: Cell::new(0),
        failure: true,
    };
    let output = crate::monitor::cli::execute(&options, &mock).await;
    assert_eq!(output.code, 1);
    assert!(output.stderr.is_empty());
    let v: Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(v["status"], "rpc_error");
    assert_eq!(v["error"]["code"], -32602);
    assert!(!output.stdout.contains("sensitive"));
}
#[test]
fn json_flag_is_opt_in_and_duplicates_are_blocked() {
    assert!(!split_json(&["--address".into(), address()]).unwrap().1);
    assert!(split_json(&["--json".into(), "--json".into()]).is_err());
}

#[test]
fn status_timeout_is_typed_without_parsing_provider_messages() {
    let timeout = crate::status_readonly::StatusTimeout;
    let v = parse(Report::failure(
        Network::Devnet,
        Operation::TransactionStatus,
        Failure::error(&timeout),
        Transport::Sdk,
        true,
        time(),
    ));
    assert_eq!(v["status"], "timeout");
    assert_eq!(timeout.to_string(), "status RPC timeout");
    // An untyped arbitrary provider message is never used to guess timeout status.
    let text = std::io::Error::other("timeout sk-private-provider-message");
    let v = parse(Report::failure(
        Network::Devnet,
        Operation::TransactionStatus,
        Failure::error(&text),
        Transport::Sdk,
        true,
        time(),
    ));
    assert_eq!(v["status"], "rpc_error");
    assert!(!v.to_string().contains("sk-private"));
}
#[test]
fn query_success_is_separate_from_transaction_execution_failure() {
    let response=serde_json::from_value(json!({"context":{"slot":99},"value":[{"slot":12,"executed":true,"err":"AccountNotFound"}]})).unwrap();
    let input = StatusReport {
        response,
        transactions: snapshot().transactions,
    };
    let v = parse(Report::statuses(
        Network::Devnet,
        &input,
        Transport::ProvidedData,
        time(),
    ));
    assert_eq!(v["status"], "success");
    assert_eq!(v["data"]["transactions"][0]["execution"], "Failed");
    assert_eq!(v["data"]["transactions"][0]["error_present"], true);
}
