# Phase 7U — Test Safety Audit

## 結果（2026-10-18 JST）

Phase 7U完了。列挙134件（library124、binary10）、実行134成功・0失敗・未実行0。
以前未実行だった129件とunsigned_testsの5件を含む。列挙操作は実行件数に加算せず、
子プロセスの同名テスト再実行も独立のテスト件数に加算しない。doctestは0件。
初期分類A51、B83、C0。B83の実行条件を確認し、今回安全確認済み合計134。
未解消の条件付き0件。新規テスト0件、既存コード変更0件。

## 調査範囲・呼び出し経路

AGENTS.mdは祖先・対象・リポジトリ内で見つからず。main、既存変更は保持。
Cargo.toml/Cargo.lock、Phase 7T、src以下の全テストと関連ヘルパーを確認。
ルートbuild.rs、tests/、examples/、benches/は存在せず、Cargo.tomlに追加target指定なし。
srcに実行可能なdoctestのコードブロックなし。Cargo列挙でも0 benchmarks/0 doctests。
依存クレート自身のテスト・doctestは今回のcargo testの対象ではない。

- src/lib.rs:59/113/141：構築・検証・wire組立。テスト鍵は固定合成バイトからメモリ内生成。
  Keyringや秘密鍵ファイルロードは呼ばない。鍵はログ・ジャーナルに保存しない。
- src/pre_sign.rs:247/316：注入したReadRpcのみを使用。Mockのprefix/fee/balanceは
  メモリ値か合成エラー。実HTTP adapter(:217)や公開RPC関数(:156/:174)へは到達しない。
- src/pre_sign/preparation.rs:120/176：テストはprepare_usingとMockを使用。
  実prepare(:171)は呼ばない。既存公開アドレス定数は文字列として扱うだけ。
- src/pre_sign/lifecycle.rs:77/325/350/367：validate_usingにMock再検証を渡す。
  responseは合成鍵の署名。実validateのRPC経路は未実行。
- src/main.rs:196以降：parse、remaining、rlo、messageだけを実行。
  テストハーネスはアプリmain/runやtestnet_readonlyを起動しない。
- persistent/tests/integration.rs:37/44/65/99：connect/send/outcomeはいずれもMock。
  sendはメモリ内wire検証とカウンタ更新だけ。TX IDもmock文字列。タイムアウトはpending future。

## Bの実行条件とファイル・子プロセス

persistent.rs:390のTempはPID＋AtomicU64で名前を作るが、既存パスを予約する方式ではない。
Drop(:399)はそのパスを再帰削除するため、共有/tmpで偶然同名パスが残る環境には
無条件の安全保証をしない。今回はmkdtempで新規作成したmode700の監査領域内に
専用TMPDIRを置き、既存データとの衝突を排除した。コードは変更しない。
同一実行中は共有atomic連番、子はPIDが異なる。crash Workspace(:30)はcreate_dir成功時のみ所有。

persistent.rs:120のinitialize/create_newとdiagnostics.rs:79のopenは、テストが渡す
専用パスだけを操作する。corruption.rs:3/17以降の改変・rename・chmodも同領域内。
本番保存先・ウォレット設定を推測してアクセスする処理はない。

crash.rs:65はcurrent_exeと--exact child_processのみを起動し、テスト専用root/modeを渡す。
:54のOwnedChildと:168はそのChildハンドルだけをkill/wait。任意PIDや外部CLIは起動しない。
persistent.rs:707のロック試験もcurrent_exe --exactだけ。親からのRIALO_7H_TEST_*、
RIALO_PERSISTENCE_LOCK_TEST混入は危険になり得るため、環境を許可リストから新規構築した。
認証環境変数を読み出し・コピーせず、子に与えた環境はPATH、TMPDIR、CARGO_NET_OFFLINE、LANGのみ。
Cargoが追加する通常のビルド環境とテスト自身が子に渡す専用変数は使用する。

persistent.rs:383のrelease_test_storeはテスト内の再open前にunlock/dropする。
SIGKILL経路には適用せず、競合検出を弱めていない。今回通常並列の1回でLockUnavailable再発0。
これは全スケジューリングでの無競合の証明ではない。

## ビルド・依存関係監査の範囲

cargo metadata --offline --lockedの初回は他OS向けanstyle-wincon未キャッシュで停止。
外部アクセスはofflineにより拒否され、ダウンロードや再試行のオンライン化はしていない。
--filter-platform x86_64-unknown-linux-gnuで解決済み依存とbuild targetを列挙した。
45個の依存build-script targetのI/O・env・外部プロセス箇所を確認した。
全依存の全関数を形式検証したものではなく、今回の既存キャッシュ利用範囲の監査。

registry基点：/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/
- rialo-cdk-0.18.1/build.rs:23/57/59は同梱schemaを読んでOUT_DIRへ定数を書くだけ。
- oid-registry-0.8.1/build.rs:3/8はsrc/load.rsと同梱OID表からOUT_DIRへ生成。
- ring-0.17.14/build.rs:292/304は環境依存のASM生成/コンパイル経路を持つ。
- blake3-1.8.7/build.rs:394はWindows限定の生成ASM削除。今回Linuxでは対象外。
- 他の主要build処理はrustc機能検出、cfg設定、同梱C/ASMコンパイルとOUT_DIR生成。

checkとtestを-vで記録し、今回依存はFreshでbuild-script実行も再コンパイルも発生しなかった。
実際に起動したテスト段階のプロセスは既存テスト実行ファイル2つとrustdoc、
そのテスト専用子プロセス。コールドビルドや異なる環境変数/targetでの安全性は未監査。
--offlineはRPC遮断の仕組みではないため、通信しない根拠は上記テストの呼出経路。
OSの全syscall監視による無通信証明は実施していない。

## コマンド・検証結果

専用環境で順に実行（エラー時停止）：
- cargo fmt --check：成功
- cargo check --offline --locked -v：成功
- cargo test --offline --locked -- --list：134件列挙、doctest0
- cargo test --offline --locked -v：124＋10＝134成功、0失敗、0 ignored
- git diff --check：成功

PATHは既存stable toolchain/binと/usr/bin:/bin、TMPDIRは監査領域/tmp、
CARGO_NET_OFFLINE=true、LANG=C.UTF-8。親環境の認証値は継承しない。
ログは /tmp/rialo-7u-audit-bpln6rhp/ のcheck.log/list.log/test.logに保存。
Cargoのtarget出力はビルド専用領域。テストデータは専用TMPDIRだけ。
テスト後TMPDIR残存0。監査開始時に既存作業ファイルをハッシュ化し、終了時変更0を確認。
.git/targetは比較対象外。新規監査文書は比較後に追加した。

警告はproc-macro-error2 v2.0.1の既存将来互換性と、Cargo global cacheの
last-useデータ保存失敗（readonly database）。後者はsandboxの書込制限で、
テスト失敗ではない。権限を拡大せず、既存コードも修正しなかった。
実ウォレット・実秘密鍵・認証情報の読取、実RPC、実送金、Faucet、commit/push、Notion記録なし。
合成鍵によるテスト署名のみ今回の明示許可に基づき実行した。

## 全テスト分類（各行の根拠＋上記共通条件）

Bは条件確認済み。分類は保守的に合成鍵を生成するfixture利用もBとした。
マクロ生成テストはマクロ呼出行を根拠に記載。

| テスト名 | 分類 | 根拠 |
| --- | --- | --- |
| `pre_sign::lifecycle::persistent::tests::abandoned_awaiting_request_cannot_resume` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:560 |
| `pre_sign::lifecycle::persistent::tests::checksum_corruption_refused` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:666 |
| `pre_sign::lifecycle::persistent::tests::concurrent_handles_refused` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:626 |
| `pre_sign::lifecycle::persistent::tests::corruption::absent_directory_not_auto_initialized` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:147 |
| `pre_sign::lifecycle::persistent::tests::corruption::conflicting_temp_file_stops` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:129 |
| `pre_sign::lifecycle::persistent::tests::corruption::directory_permissions_rejected` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:108 |
| `pre_sign::lifecycle::persistent::tests::corruption::duplicate_registration_with_valid_checksum` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:82 |
| `pre_sign::lifecycle::persistent::tests::corruption::file_permissions_rejected` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:102 |
| `pre_sign::lifecycle::persistent::tests::corruption::invalid_transition_with_valid_checksum` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:75 |
| `pre_sign::lifecycle::persistent::tests::corruption::malformed_json_redacted` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:42 |
| `pre_sign::lifecycle::persistent::tests::corruption::matching_temp_file_stops` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:122 |
| `pre_sign::lifecycle::persistent::tests::corruption::missing_fields_rejected` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:59 |
| `pre_sign::lifecycle::persistent::tests::corruption::missing_file_not_initialized` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:114 |
| `pre_sign::lifecycle::persistent::tests::corruption::normal_state_preserved_and_id_blocked` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:139 |
| `pre_sign::lifecycle::persistent::tests::corruption::truncated_json_preserved` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:52 |
| `pre_sign::lifecycle::persistent::tests::corruption::unknown_field_never_disclosed` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:65 |
| `pre_sign::lifecycle::persistent::tests::corruption::unknown_protocol_rejected` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:95 |
| `pre_sign::lifecycle::persistent::tests::corruption::unknown_schema_rejected` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/corruption.rs:89 |
| `pre_sign::lifecycle::persistent::tests::crash::child_process` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:8 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_after_response_sync` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:214 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_after_signed_sync` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:221 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_before_response_sync` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:208 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_before_response_write` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:196 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_during_validation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:227 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_partial_response_json` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:202 |
| `pre_sign::lifecycle::persistent::tests::crash::kill_partial_signed_json` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:219 |
| `pre_sign::lifecycle::persistent::tests::crash::missing_file_rejected_without_recreation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:244 |
| `pre_sign::lifecycle::persistent::tests::crash::permissions_rejected_without_modification` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:255 |
| `pre_sign::lifecycle::persistent::tests::crash::save_failure_then_fresh_process_rejects_reuse` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/crash.rs:234 |
| `pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:707 |
| `pre_sign::lifecycle::persistent::tests::duplicate_and_different_message_id_refused` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:532 |
| `pre_sign::lifecycle::persistent::tests::expired_survives_restart` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:510 |
| `pre_sign::lifecycle::persistent::tests::final_save_failure_withholds_wire_and_restart_blocks_retry` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:684 |
| `pre_sign::lifecycle::persistent::tests::integration::connection_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:196 |
| `pre_sign::lifecycle::persistent::tests::integration::construction_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:200 |
| `pre_sign::lifecycle::persistent::tests::integration::dummy_signer_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:202 |
| `pre_sign::lifecycle::persistent::tests::integration::execution_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:226 |
| `pre_sign::lifecycle::persistent::tests::integration::fee_exceeds_limit` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:199 |
| `pre_sign::lifecycle::persistent::tests::integration::insufficient_balance_including_fee` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:197 |
| `pre_sign::lifecycle::persistent::tests::integration::missing_fee` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:198 |
| `pre_sign::lifecycle::persistent::tests::integration::mock_send_failure_no_retry` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:214 |
| `pre_sign::lifecycle::persistent::tests::integration::offline_pipeline_success` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:195 |
| `pre_sign::lifecycle::persistent::tests::integration::save_failure_blocks_send` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:208 |
| `pre_sign::lifecycle::persistent::tests::integration::timeout_no_retry` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:225 |
| `pre_sign::lifecycle::persistent::tests::integration::unknown_result_no_retry` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent/tests/integration.rs:220 |
| `pre_sign::lifecycle::persistent::tests::missing_and_corrupt_files_refused` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:634 |
| `pre_sign::lifecycle::persistent::tests::normal_save_reload_and_signed_replay` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:462 |
| `pre_sign::lifecycle::persistent::tests::permissions_restricted_and_insecure_file_refused` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:650 |
| `pre_sign::lifecycle::persistent::tests::registration_io_failure_stops_and_poisons` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:576 |
| `pre_sign::lifecycle::persistent::tests::rejected_survives_restart` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:487 |
| `pre_sign::lifecycle::persistent::tests::request_not_exposed_on_save_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:595 |
| `pre_sign::lifecycle::persistent::tests::response_not_processed_on_claim_save_failure` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle/persistent.rs:613 |
| `pre_sign::lifecycle::tests::cancelled_validation_is_terminal` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:570 |
| `pre_sign::lifecycle::tests::clock_failure_rejects_response` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:538 |
| `pre_sign::lifecycle::tests::expired_checks_block_request_creation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:530 |
| `pre_sign::lifecycle::tests::expiry_during_assembly_never_returns_wire` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:550 |
| `pre_sign::lifecycle::tests::fee_excess_blocks_request_creation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:515 |
| `pre_sign::lifecycle::tests::ids_are_distinct_within_process` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:584 |
| `pre_sign::lifecycle::tests::network_mismatch_blocks_request_creation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:507 |
| `pre_sign::lifecycle::tests::rejects_amount_tampering` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:470 |
| `pre_sign::lifecycle::tests::rejects_double_response_and_signed_request_reuse` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:490 |
| `pre_sign::lifecycle::tests::rejects_expired_response_and_reuse` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:480 |
| `pre_sign::lifecycle::tests::rejects_fee_display_tampering` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:476 |
| `pre_sign::lifecycle::tests::rejects_hash_mismatch` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:447 |
| `pre_sign::lifecycle::tests::rejects_invalid_signature` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:434 |
| `pre_sign::lifecycle::tests::rejects_message_tampering` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:464 |
| `pre_sign::lifecycle::tests::rejects_missing_signature` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:450 |
| `pre_sign::lifecycle::tests::rejects_network_tampering` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:473 |
| `pre_sign::lifecycle::tests::rejects_protocol_version_mismatch` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:444 |
| `pre_sign::lifecycle::tests::rejects_public_key_mismatch` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:437 |
| `pre_sign::lifecycle::tests::rejects_recipient_tampering` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:467 |
| `pre_sign::lifecycle::tests::rejects_rejected_session_revalidation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:498 |
| `pre_sign::lifecycle::tests::rejects_request_id_mismatch` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:440 |
| `pre_sign::lifecycle::tests::rpc_failure_blocks_request_creation` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:523 |
| `pre_sign::lifecycle::tests::valid_request_fields_come_from_message_and_checks` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:377 |
| `pre_sign::lifecycle::tests::valid_response_builds_local_wire_and_signature_verifies` | B：合成鍵/一時領域/子プロセス条件確認済み | src/pre_sign/lifecycle.rs:400 |
| `pre_sign::preparation::tests::config_mismatch_blocks` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:323 |
| `pre_sign::preparation::tests::fee_limit_exceeded` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:300 |
| `pre_sign::preparation::tests::insufficient_balance_for_estimate` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:293 |
| `pre_sign::preparation::tests::insufficient_balance_for_limit` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:284 |
| `pre_sign::preparation::tests::invalid_inputs` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:262 |
| `pre_sign::preparation::tests::missing_fee_blocks` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:307 |
| `pre_sign::preparation::tests::rpc_failure_blocks` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:316 |
| `pre_sign::preparation::tests::success_and_never_signed_or_broadcast` | A：純粋処理またはメモリMock | src/pre_sign/preparation.rs:250 |
| `pre_sign::tests::backward_and_divergent_clocks_stop` | A：純粋処理またはメモリMock | src/pre_sign.rs:545 |
| `pre_sign::tests::clock_read_failure_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:555 |
| `pre_sign::tests::config_change_during_queries_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:467 |
| `pre_sign::tests::fee_over_limit_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:420 |
| `pre_sign::tests::initial_check_expiring_during_rpc_is_rejected` | A：純粋処理またはメモリMock | src/pre_sign.rs:696 |
| `pre_sign::tests::insufficient_balance_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:425 |
| `pre_sign::tests::invalid_clock_and_ttl_stop_before_rpc` | A：純粋処理またはメモリMock | src/pre_sign.rs:522 |
| `pre_sign::tests::invalid_fee_data_is_rejected_by_cdk_response_type` | A：純粋処理またはメモリMock | src/pre_sign.rs:438 |
| `pre_sign::tests::matching_prefix_and_exact_message_succeed` | A：純粋処理またはメモリMock | src/pre_sign.rs:379 |
| `pre_sign::tests::missing_fee_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:414 |
| `pre_sign::tests::modified_message_stops_before_rpc` | A：純粋処理またはメモリMock | src/pre_sign.rs:431 |
| `pre_sign::tests::overflowing_total_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:460 |
| `pre_sign::tests::prefix_mismatch_stops` | A：純粋処理またはメモリMock | src/pre_sign.rs:400 |
| `pre_sign::tests::result_expires_exactly_at_deadline` | A：純粋処理またはメモリMock | src/pre_sign.rs:513 |
| `pre_sign::tests::result_valid_before_deadline` | A：純粋処理またはメモリMock | src/pre_sign.rs:504 |
| `pre_sign::tests::revalidation_cannot_renew_old_result_that_expires_during_rpc` | A：純粋処理またはメモリMock | src/pre_sign.rs:705 |
| `pre_sign::tests::revalidation_refreshes_increased_fee_within_original_limit` | A：純粋処理またはメモリMock | src/pre_sign.rs:618 |
| `pre_sign::tests::revalidation_rejects_amount_change` | A：純粋処理またはメモリMock | src/pre_sign.rs:603 |
| `pre_sign::tests::revalidation_rejects_corrupt_or_missing_report_data` | A：純粋処理またはメモリMock | src/pre_sign.rs:658 |
| `pre_sign::tests::revalidation_rejects_fee_limit_excess` | A：純粋処理またはメモリMock | src/pre_sign.rs:632 |
| `pre_sign::tests::revalidation_rejects_insufficient_balance` | A：純粋処理またはメモリMock | src/pre_sign.rs:639 |
| `pre_sign::tests::revalidation_rejects_message_change` | A：純粋処理またはメモリMock | src/pre_sign.rs:595 |
| `pre_sign::tests::revalidation_rejects_network_change` | A：純粋処理またはメモリMock | src/pre_sign.rs:611 |
| `pre_sign::tests::revalidation_rejects_public_key_change` | A：純粋処理またはメモリMock | src/pre_sign.rs:607 |
| `pre_sign::tests::revalidation_rejects_recipient_change` | A：純粋処理またはメモリMock | src/pre_sign.rs:599 |
| `pre_sign::tests::revalidation_rejects_rpc_errors_and_missing_fee` | A：純粋処理またはメモリMock | src/pre_sign.rs:646 |
| `pre_sign::tests::revalidation_success_with_unchanged_state` | A：純粋処理またはメモリMock | src/pre_sign.rs:715 |
| `pre_sign::tests::rpc_errors_stop` | A：純粋処理またはメモリMock | src/pre_sign.rs:406 |
| `tests::rejects_invalid_addresses_and_same_recipient` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:232 |
| `tests::rejects_invalid_and_malformed_signatures` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:175 |
| `tests::rejects_missing_and_duplicate_signatures` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:212 |
| `tests::rejects_non_testnet_endpoints` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:260 |
| `tests::rejects_signatures_for_different_message_or_encoding` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:274 |
| `tests::rejects_wrong_public_key_and_wrong_signing_key` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:189 |
| `tests::valid_signature_round_trips_exact_message` | B：合成鍵/一時領域/子プロセス条件確認済み | src/lib.rs:150 |
| `unsigned_tests::deterministic_and_sensitive_to_all_transaction_inputs` | A：純粋処理またはメモリMock | src/unsigned_tests.rs:48 |
| `unsigned_tests::fee_payload_is_message_not_unsigned_envelope` | A：純粋処理またはメモリMock | src/unsigned_tests.rs:73 |
| `unsigned_tests::invalid_or_missing_address_and_network_rejected` | A：純粋処理またはメモリMock | src/unsigned_tests.rs:99 |
| `unsigned_tests::malformed_unsigned_envelopes_rejected` | A：純粋処理またはメモリMock | src/unsigned_tests.rs:89 |
| `unsigned_tests::unsigned_fields_and_canonical_wire` | A：純粋処理またはメモリMock | src/unsigned_tests.rs:18 |
| `preparation_cli_tests::preparation_mode_requires_all_explicit_fields` | A：純粋処理またはメモリMock | src/main.rs:349 |
| `testnet_tests::explicit_testnet_readonly_mode` | A：純粋処理またはメモリMock | src/main.rs:314 |
| `testnet_tests::testnet_rejects_mutation_flags_and_invalid_address` | A：純粋処理またはメモリMock | src/main.rs:331 |
| `tests::amount_override_is_rejected` | A：純粋処理またはメモリMock | src/main.rs:231 |
| `tests::balance_boundaries` | A：純粋処理またはメモリMock | src/main.rs:275 |
| `tests::canonical_unsigned_transfer` | A：純粋処理またはメモリMock | src/main.rs:283 |
| `tests::inputs_fail_closed` | A：純粋処理またはメモリMock | src/main.rs:258 |
| `tests::network_override_cannot_enable_sending` | A：純粋処理またはメモリMock | src/main.rs:238 |
| `tests::ordinary_invocation_is_prepare_only` | A：純粋処理またはメモリMock | src/main.rs:201 |
| `tests::send_flags_fail_before_rpc_or_key_access` | A：純粋処理またはメモリMock | src/main.rs:216 |

## 残るリスク・次の最小作業

この監査はオフラインテストの安全性と成功を確認したもので、CLI/CDK互換性や
実送金成功を証明しない。署名payloadのCLI比較資料、手数料0の原因は未解決。
次の最小作業は配布元に対象CLI版の鍵/RPC不要な未署名fixtureまたは正式export手順を
照会すること。今回は照会を送らず、Phase 7Uで停止する。
