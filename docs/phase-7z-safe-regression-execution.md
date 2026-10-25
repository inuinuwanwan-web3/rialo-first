# Safe Regression execution log

2026-10-25。実行コマンドと生ログ。子プロセスの内部実行は独立テスト件数に加算しない。

```text
$ cargo test --offline --locked --lib -- --exact monitor::cli::tests::error_exit_code_redacted_json monitor::cli::tests::height_and_config_json monitor::cli::tests::invalid_address monitor::cli::tests::invalid_tx monitor::cli::tests::normal_balance monitor::cli::tests::rpc_and_options_validation monitor::cli::tests::timeout_exit_code monitor::cli::tests::tx_absent_unknown monitor::cli::tests::tx_confirmed monitor::cli::tests::tx_failed monitor::cli::tests::tx_unexecuted_unknown monitor::cli::tests::zero_balance monitor::query::tests::conflicting_duplicate_rejected monitor::query::tests::detail_failure monitor::query::tests::detail_id_mismatch monitor::query::tests::detail_success_and_block_evidence monitor::query::tests::duplicate_transaction monitor::query::tests::empty_history monitor::query::tests::explicit_pagination_requests monitor::query::tests::finality_remains_unspecified monitor::query::tests::history_shape_and_count_rejected monitor::query::tests::invalid_inputs_no_query monitor::query::tests::multiple_transactions monitor::query::tests::nonadvancing_cursor_rejected monitor::query::tests::null_is_not_absence_proof monitor::query::tests::recorded_live_empty_history_shape monitor::query::tests::rpc_errors_and_timeouts monitor::tests::ambiguous_unexecuted_is_unknown monitor::tests::balance_normal monitor::tests::balance_zero monitor::tests::duplicate_tx monitor::tests::invalid_hash_before_rpc monitor::tests::malformed_status_count monitor::tests::no_requested_tx_is_not_empty_history monitor::tests::recorded_live_status_remains_unknown monitor::tests::rpc_error monitor::tests::rpc_timeout monitor::tests::tx_failed monitor::tests::tx_success monitor::tests::tx_unknown monitor::tests::unknown_reasons_preserve_ambiguity pre_sign::lifecycle::persistent::tests::concurrent_handles_refused pre_sign::lifecycle::persistent::tests::corruption::absent_directory_not_auto_initialized pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user pre_sign::lifecycle::persistent::tests::integration::connection_failure pre_sign::lifecycle::persistent::tests::missing_and_corrupt_files_refused pre_sign::lifecycle::persistent::tests::permissions_restricted_and_insecure_file_refused pre_sign::preparation::tests::invalid_inputs pre_sign::tests::invalid_fee_data_is_rejected_by_cdk_response_type unsigned_tests::invalid_or_missing_address_and_network_rejected
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.33s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
     Running unittests src/lib.rs (target/debug/deps/rialo_first-6d8c47c011be4a40)

running 50 tests
test monitor::cli::tests::invalid_address ... ok
test monitor::cli::tests::invalid_tx ... ok
test monitor::cli::tests::error_exit_code_redacted_json ... ok
test monitor::cli::tests::height_and_config_json ... ok
test monitor::cli::tests::rpc_and_options_validation ... ok
test monitor::cli::tests::normal_balance ... ok
test monitor::cli::tests::tx_absent_unknown ... ok
test monitor::cli::tests::tx_confirmed ... ok
test monitor::cli::tests::tx_failed ... ok
test monitor::cli::tests::tx_unexecuted_unknown ... ok
test monitor::cli::tests::zero_balance ... ok
test monitor::query::tests::empty_history ... ok
test monitor::query::tests::detail_failure ... ok
test monitor::query::tests::conflicting_duplicate_rejected ... ok
test monitor::query::tests::detail_success_and_block_evidence ... ok
test monitor::query::tests::detail_id_mismatch ... ok
test monitor::query::tests::duplicate_transaction ... ok
test monitor::query::tests::invalid_inputs_no_query ... ok
test monitor::query::tests::history_shape_and_count_rejected ... ok
test monitor::query::tests::finality_remains_unspecified ... ok
test monitor::query::tests::explicit_pagination_requests ... ok
test monitor::query::tests::multiple_transactions ... ok
test monitor::query::tests::recorded_live_empty_history_shape ... ok
test monitor::query::tests::nonadvancing_cursor_rejected ... ok
test monitor::query::tests::null_is_not_absence_proof ... ok
test monitor::tests::ambiguous_unexecuted_is_unknown ... ok
test monitor::tests::balance_normal ... ok
test monitor::tests::balance_zero ... ok
test monitor::tests::duplicate_tx ... ok
test monitor::tests::no_requested_tx_is_not_empty_history ... ok
test monitor::tests::invalid_hash_before_rpc ... ok
test monitor::tests::recorded_live_status_remains_unknown ... ok
test monitor::tests::malformed_status_count ... ok
test monitor::tests::rpc_error ... ok
test monitor::tests::tx_success ... ok
test monitor::tests::tx_unknown ... ok
test monitor::tests::tx_failed ... ok
test pre_sign::lifecycle::persistent::tests::corruption::absent_directory_not_auto_initialized ... ok
test pre_sign::preparation::tests::invalid_inputs ... ok
test pre_sign::tests::invalid_fee_data_is_rejected_by_cdk_response_type ... ok
test unsigned_tests::invalid_or_missing_address_and_network_rejected ... ok
test monitor::tests::unknown_reasons_preserve_ambiguity ... ok
test monitor::cli::tests::timeout_exit_code ... ok
test monitor::tests::rpc_timeout ... ok
test pre_sign::lifecycle::persistent::tests::concurrent_handles_refused ... ok
test pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user ... ok
test monitor::query::tests::rpc_errors_and_timeouts ... ok
test pre_sign::lifecycle::persistent::tests::missing_and_corrupt_files_refused ... ok
test pre_sign::lifecycle::persistent::tests::integration::connection_failure ... ok
test pre_sign::lifecycle::persistent::tests::permissions_restricted_and_insecure_file_refused ... ok

test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 116 filtered out; finished in 0.04s


$ cargo test --offline --locked --bin rialo-first -- --exact monitor_cli_tests::explicit_monitor_and_existing_modes testnet_tests::explicit_testnet_readonly_mode testnet_tests::testnet_rejects_mutation_flags_and_invalid_address tests::amount_override_is_rejected tests::balance_boundaries tests::inputs_fail_closed tests::network_override_cannot_enable_sending tests::ordinary_invocation_is_prepare_only tests::send_flags_fail_before_rpc_or_key_access
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.29s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
     Running unittests src/main.rs (target/debug/deps/rialo_first-d2d8bdcb9383cf09)

running 9 tests
test testnet_tests::explicit_testnet_readonly_mode ... ok
test monitor_cli_tests::explicit_monitor_and_existing_modes ... ok
test tests::amount_override_is_rejected ... ok
test testnet_tests::testnet_rejects_mutation_flags_and_invalid_address ... ok
test tests::balance_boundaries ... ok
test tests::inputs_fail_closed ... ok
test tests::network_override_cannot_enable_sending ... ok
test tests::ordinary_invocation_is_prepare_only ... ok
test tests::send_flags_fail_before_rpc_or_key_access ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s


$ cargo test --offline --locked --test phase_7z_monitor -- --exact budget_retains_cursor_and_short_page_terminates cli_json_and_errors_reuse_existing_monitor conflicts_and_cursor_cycles_are_errors details_preserve_error_and_height invalid_inputs_do_not_call_rpc missing_and_incomplete_details_do_not_imply_success nonempty_history_preserves_provider_order_and_fields pagination_deduplicates_boundary_and_finishes rpc_errors_timeouts_and_empty_history synthetic_sdk_shapes_and_status_reasons typed_history_transaction_error_is_preserved
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.29s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
     Running tests/phase_7z_monitor.rs (target/debug/deps/phase_7z_monitor-443088cda737db63)

running 11 tests
test nonempty_history_preserves_provider_order_and_fields ... ok
test typed_history_transaction_error_is_preserved ... ok
test conflicts_and_cursor_cycles_are_errors ... ok
test pagination_deduplicates_boundary_and_finishes ... ok
test budget_retains_cursor_and_short_page_terminates ... ok
test invalid_inputs_do_not_call_rpc ... ok
test synthetic_sdk_shapes_and_status_reasons ... ok
test details_preserve_error_and_height ... ok
test cli_json_and_errors_reuse_existing_monitor ... ok
test missing_and_incomplete_details_do_not_imply_success ... ok
test rpc_errors_timeouts_and_empty_history ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s


$ cargo test --offline --locked --example phase_7y_readonly -- --exact tests::candidate_selection_is_bounded_and_explicit
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.31s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`
     Running unittests examples/phase_7y_readonly.rs (target/debug/examples/phase_7y_readonly-3c7dfad43f530726)

running 1 test
test tests::candidate_selection_is_bounded_and_explicit ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


$ cargo fmt --check

$ git diff --check

$ cargo build --offline --locked
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.29s
warning: the following packages contain code that will be rejected by a future version of Rust: proc-macro-error2 v2.0.1
note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1`

```
