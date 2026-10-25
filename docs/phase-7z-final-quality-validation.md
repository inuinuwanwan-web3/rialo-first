# Phase 7Z — Final quality validation

2026-10-25。基準HEAD a37fd7cf8f6636c70c7013c06504ddc77edd72b7。

**結論：安全なRead-Only Monitor検証の範囲で品質改善作業は終了可。**
A〜FはPASS6 / FAIL0 / UNKNOWN0。ただしこの表は「互換性への対応と未解決事項の影響評価」の
完了判定であり、上流SDKの全メソッド完全互換や全履歴保持を認定するものではない。
Phase 7Zの総合記録は引き続き **PARTIAL PASS（既知制約あり）** とする。

実装・検証済みの事実と、未確認のサーバー仕様/運用上の条件を以下で区別する。
追加RPC/Web照会なし。SDK・依存・本番Monitorの変更なし。新Phaseへの移行なし。
既存の検証レポート/JSONを削除・上書きしていない。

## 1. 使用した証拠

- phase-7z-live-testnet-results.json：既存公開TX5件の詳細/SDK状態/非空履歴2ページ。
- phase-7z-live-testnet-validation.md：既存live検証の範囲と制約。
- phase-7z-bounded-history-results.json：前回承認された2要求によるbefore/until区間終端の実測。
- phase-7z-remaining-compatibility-validation.md：SDK便利メソッド不適合と残るUNKNOWN。
- tests/phase_7z_monitor.rs、src/monitor/query.rsの既存テスト：重複・cursor・エラー等。

今回の個別出力/時刻/入力hash/通信遮断証拠は
[品質検証結果JSON](phase-7z-final-quality-results.json)へ新規保存した。
過去のlive応答を再生した成功と、今回の新しいlive照会の成功を混同しない。今回は外部照会0件。

## 2. SDK不一致の影響と対応

### 確認した処理（SDKの基点はCargo registryのrialo-cdk-0.18.1）

| ファイル・行 | 処理 | 結果 |
|---|---|---|
| SDK src/rpc/client.rs:727 | get_signatures_for_address | context/value応答をVec<generated::SignatureInfo>へ直接復号する不一致 |
| SDK src/rpc/client.rs:381 | call_with_json | Monitorが使用する既存HTTP経路 |
| SDK依存rialo-api-types-0.18.1/src/messages/get_signatures_for_address.rs:71,88 | 公式レスポンス/SignatureInfo | context/valueと型付きerrを定義 |
| src/monitor/query.rs:22 | Query::request | getSignaturesForAddressへ公式requestを送る |
| src/monitor/query.rs:36 | QueryRpc for HttpRpcClient | call_with_jsonへ委譲。問題の便利メソッドを呼ばない |
| src/monitor/query.rs:112 | history_page | 公式GetSignaturesForAddressResponseを直接復号 |
| src/monitor/query.rs:58 | history_pages | 全ページをhistory_page経由で取得。重複/循環/全体timeout/予算を適用 |
| src/monitor/query.rs:173 | transaction_detail | 詳細は公式GetTransactionResponseと先頭Signature照合 |
| src/monitor.rs:82 | observe_status | executed/errと理由を分類。None/未実行をUnknownに維持 |
| src/monitor/cli.rs:73,99 | execute/run | balance/statusのsnapshot経路。履歴便利メソッドは使用しない |
| examples/phase_7y_readonly.rs:33,121 | Recorder::query / history呼出 | QueryRpcとhistory_pageへ委譲 |
| scripts/phase7z_saved_response_replay.rs:50 | verify | 実応答の型復号・変換・SDK不一致・既存Monitor成功をオフライン再現 |

src/examples/tests/scriptsのRust呼出箇所を調べ、問題のSDK
get_signatures_for_address便利メソッドを直接呼ぶMonitor経路は存在しなかった。
履歴取得はすべて既存の公式context/value型による回避処理を通る。
CLIのaddress_history_supported=falseはそのsnapshotが履歴を取得しないという既存仕様で、
便利メソッドを呼ぶ未対応経路ではない。

**実運用の既存Monitor経路には不一致の影響なし。既存回避処理で対応済み。**
よって本番修正/fallback/SDK直書換/依存更新は不要と判断した。
SDK便利メソッド自体を互換化したわけではなく、外部利用者がMonitorを迂回して直接呼ぶ場合の
不適合は残る。これは前回報告のFAILを隠してPASSに変更するものではない。

正常系は保存済み公式応答のMonitor再生成功、異常系は同じwrapperをSDKの期待Vecに
復号すると失敗することと、既存Monitorがbare配列/欠損を拒否することを確認。
SDKの全APIを検証したとは主張しない。

## 3. 今回の変更と安全性

本番コードと既存189件のCargoテスト定義・allowlistは一切変更なし。
変更したのは前回作成した独立診断のscripts/phase7z_saved_response_replay.rsへの次の4テスト追加。

| 行番号 | 新規テスト・補助処理 | 追加理由 |
|---|---|---|
| 210 | quality_tests module | Cargo既存inventoryを変えず、監査した独立テストだけを実行する |
| 219 | malformed_or_required_missing_later_page_never_returns_partial_success | 成功1ページ後に壊れた応答/必須欠損を返す12ケース。既存は主に最初のページのみ |
| 272 | absent_optional_fields_on_later_page_remain_valid | 任意err/blockTimeの省略を必須欠損と混同して拒否しない |
| 297,335 | FaultAfterFirst / check_later_failure | 2ページ目だけをRPC error/pending futureにするメモリMock |
| 364 | second_page_rpc_error_stops_without_partial_success_or_retry | 2ページ目の失敗を部分成功として返さず、再試行しないことを確認 |
| 369 | second_page_timeout_cancels_pending_request_without_partial_success | 成功ページ後のtimeoutとpending futureのDropによる取消を確認 |

3ページ目以降への誤った再試行はMockでpanicさせ、要求数2を照合。
必須欠損等の12ケースは1つの登録テスト内で回すため、12件のテストとして水増ししない。
不足していたのは「中途ページでの異常停止」と「必須/任意欠損の区別」であり、
既存の正常ページ・重複・cursor試験を重複追加しなかった。

追加MockはJSON/Cell/RefCell/pending futureのみ。HTTP client・TX builder・鍵・署名機能なし。
include_strは既存のsyntheticレスポンスfixtureだけを読む。
既存Runnerのblock_networkでフィルタをロードしてprobe後、コンパイラとテストへ継承。
余分なFDは閉じ、テストには専用temp HOME/TMPDIRと最小環境だけを渡した。
既存のSDK/Monitorと同一のコンパイル済みrlibをrustc --testでリンクした。
これら4件は手動監査の完全名allowlistで実行し、既存Runnerの71件へ自動追加していない。

## 4. 要求10条件の期待値と実測

| 条件 | 期待動作 | 実際の結果/主な根拠 | 判定 |
|---|---|---|---|
| 1 正常な複数ページ | beforeを渡し件数/順序/ID/heightを保持 | 既存pagination_deduplicates_boundary_and_finishes（tests/phase_7z_monitor.rs:163）と保存実応答再生で一致 | PASS |
| 2 空ページ | entries空、next_beforeなし、追加照会しない | 既存rpc_errors_timeouts_and_empty_historyとquery::tests::empty_historyを再利用 | PASS |
| 3 最終ページ | 空/短ページで停止、満杯なら次候補を維持 | 既存budget_retains_cursor_and_short_page_terminates（同:189）、保存された有限区間live終端、再生診断で一致 | PASS |
| 4 重複TX | 内容一致なら除去、有効な別TXを残す、矛盾はエラー | 既存pagination_deduplicates…/conflicts_and_cursor_cycles_are_errors（同:204）を再利用 | PASS |
| 5 不正cursor | before/untilを通信前に拒否 | 既存invalid_inputs_do_not_call_rpc（同:250）、新規の不正応答Signatureケースもエラー | PASS |
| 6 cursor循環 | 集約をエラーで止める | 既存conflicts_and_cursor_cycles…と再生診断で拒否 | PASS |
| 7 RPC timeout | Errを返し継続しない | 既存初回timeoutに加え、新規2ページ目timeoutで要求2回・pending future取消を確認 | PASS |
| 8 RPC error | Errを返し部分成功/再試行にしない | 既存初回errorに加え、新規2ページ目errorで要求2回・Errを確認 | PASS |
| 9 不正response形式 | bare配列/null/不正型を空履歴へ変換しない | 既存query::tests::history_shape_and_count_rejected（src/monitor/query.rs:319）と新規中途ページ試験で拒否 | PASS |
| 10 部分的欠損 | 必須欠損はErr、SDKが任意とするフィールド省略は許容 | 新規2テストで必須context/slot/value/signature/height欠損を拒否、err/blockTime省略は正常終了 | PASS |

新規の不正12ケース：bare配列、null、value欠損、context欠損、slot欠損、value=null、
slotが文字列、Signature欠損、height欠損、不正Signature、height=null、要求件数超過。
全ケースで1ページ目の取得成功を「全体の成功」へ変換せず2要求で停止。
Err時に途中の履歴を正常なHistoryBatchとして返さないことを実測した。

10条件の判定はクライアント処理のオフライン検証。実サーバーから異常応答が発生したとの主張ではない。
既存live証拠があるのは公開TX成功/非空ページ/有限区間終端等に限る。

## 5. テスト集計

| 区分 | 定義数 | 実行 | 成功 | 失敗 | 除外 | 実行対象の未実行 |
|---|---:|---:|---:|---:|---:|---:|
| 既存Cargoテスト | 189 | 71 | 71 | 0 | 118（D） | 0 |
| 追加の独立Rustテスト | 4 | 4 | 4 | 0 | 0 | 0 |
| 合計 | 193 | 75 | 75 | 0 | 118 | 0 |

既存189件の定義/71件allowlistは不変。追加4件は別のrustc --testバイナリに登録されたテスト。
11個の保存応答診断チェックも11成功・0失敗だが、登録テスト数75件へ加算しない。
Runner自体のPython9テストはRunner無変更のため今回対象外で未実行。過去成功を今回加算しない。

既存回帰はscripts/phase7z_regression.pyでネットワーク遮断を確認して実行し、
71件の完全名/個別ok出力/exit codeと集計を照合。D118件は実行しない。
新規4件も完全名の指定集合とlibtestの個別ok行が一致し、ignored/未実行を成功扱いしない。
SDK便利メソッドの形式不適合を期待どおり再現した診断の成功と、互換性上の不適合は別の判定。

最終チェック：cargo fmt --check、独立診断のrustfmt --check --edition 2024、git diff --check成功。
依存追加・SDK書換・本番コード修正はない。既存の検証レポートとJSONはそのまま維持。

## 6. A〜F終了評価

| 項目 | 判定 | 適用範囲と証拠 |
|---|---|---|
| A 公開TXの取得と状態判定 | PASS | 保存済みlive公開TX5件の成功証拠と今回の同データ再生。現在への追加照会なし |
| B SDK互換性への対応 | PASS | 既存Monitorの全履歴経路が回避処理を通る。SDK便利メソッド自体の不適合は残る |
| C ページネーション正常系 | PASS | 保存live有限区間終端、非空2ページ、既存正常系テスト |
| D ページネーション異常系 | PASS | 上記10条件、追加4テストと既存試験の再利用 |
| E オフライン回帰 | PASS | 既存71件維持、新規4件成功、失敗0 |
| F 未解決事項の影響評価 | PASS | 残る制約を下記に限定し、Monitorの安全動作に対する影響と禁止する主張を明記 |

**A〜F：PASS6 / FAIL0 / UNKNOWN0。** これは品質対応/影響評価のチェックリスト。
以前の16項目のFAIL1/UNKNOWN4という証拠を取り消すものではない。
未解決の技術事項は、SDK便利メソッドの既知不適合1件と、次のUNKNOWN4件を維持する。

| 残るUNKNOWN | 安全動作への影響評価 | 引き続き主張しないこと |
|---|---|---|
| 保持期間・検索可能範囲・全履歴終端 | read-onlyで取得できた範囲を返し、予算/timeout/エラー停止を守る用途は継続可能 | 全期間完全履歴、過去TX永続照会保証 |
| 古いTXがNot Foundになる根因 | Not Found/RPC errorをSuccess/Failureへ補完せず、Unknown/取得エラーとして扱う | 過去の実行失敗、削除、pruning、リセットの断定 |
| 過去と現在の同一ledger継続性 | 観測時刻とendpointの範囲で扱い、過去と現在を同一chainと仮定した確認数を計算しない | 高さ差だけでのchain同一性/Finality認定 |
| 元のstatus wire payload全体 | SDK変換後のexecuted/err判定は実測済み。slotの置換を認識して運用可能 | 元slotの復元、SDKの全wireフィールドの完全な検証 |

上記は、取得不能をUnknown/エラーで止める**限定的な読み取り運用**を妨げないと判断。
これは永続archiveサービスとしての安全性/完全性を証明したという意味ではない。
実際に外部利用者がそれらの強い保証を必要とする場合は、別途公式仕様確認が必要。

## 7. 終了可否と残作業

- 本目的の品質改善作業は終了可。必要な異常系の不足を埋め、運用経路を保護した既存設計を確認した。
- Phase 7Z総合はPARTIAL PASSのまま。全UNKNOWNを解消するまで反復調査する必要はない。
- 必須の追加実装修正は0件、同じ検証を繰り返す必要もない。
- 任意の公式保持仕様/ledger情報との照合は、証拠が得られた場合1〜2時間程度。
  公式回答待ちの暦時間は推定不能。今回照会やメッセージ送信はしない。
- 任意のSDK上流不適合の報告案整理は0.5〜1時間。依存変更は別途監査が必要。
- 次は成果レビューと既知制約の運用引き継ぎ。Phase 8の開発/移行は開始しない。

秘密情報アクセス、TX作成/署名/送信、Faucet、AGP監視操作、外部RPC、commit/push、Notion操作なし。
