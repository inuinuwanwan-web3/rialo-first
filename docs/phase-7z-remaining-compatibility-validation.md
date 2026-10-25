# Phase 7Z — Remaining compatibility validation

2026-10-25。基準HEAD a37fd7cf8f6636c70c7013c06504ddc77edd72b7。
**総合判定：PARTIAL PASS**。既存Monitorと189件の定義・71件allowlistを維持。
新Phase・commit/push・Notion記録なし。既存の署名調査の未追跡ファイルには触れていない。

## 利用した証拠と検証範囲

既存のlive-testnet-results.json、live-testnet-validation.md、
testnet-compatibility-investigation.mdを優先し、前回の公開TX5件・履歴2ページを再利用。
追加Web検索や過去の公開TXへの再照会は行わなかった。
導入済みCDK 0.18.1とAPI型の確認は、既知の変換処理と検索条件に限定した。

- `scripts/phase7z_saved_response_replay.rs`：保存応答を既存QueryRpcへ注入するオフライン診断。
  HttpRpcClientを作らず、既存transaction_detail/history_pages/observe_statusを使用。
- コンパイルと診断実行の両方に既存Runnerのblock_networkを適用し、socket EPERMを確認。
  既存コンパイル済みrlibをrustcでリンクしただけで、依存追加・Cargo target追加はない。
- [再生チェック・今回の回帰結果](phase-7z-saved-response-replay-results.json)：入力SHA-256、
  11チェックの証拠種別、71件の個別成功出力、118件の除外一覧。
- [追加の限定区間RPC証拠](phase-7z-bounded-history-results.json)：要求・応答・日時・既存Monitor集約結果。

11チェックのうち、保存応答のそのままの再生と、空/短ページ/重複/循環を合成した試験を
evidenceフィールドで区別した。合成した空ページを実サーバー由来とは記録しない。
元の公開TX5件/履歴2ページの証拠は変更していない。

## ページネーション

### オフライン再生

- 保存済み詳細5件をtransaction_detailで再生し、Signature・height・metaを保存結果と照合。
- 保存済み履歴のcontext/valueを公式GetSignaturesForAddressResponseで復号。
- history_pagesを2ページ予算で実行し、4件・重複0・保存済みnext_beforeを再現。
  実際に生成した2ページ分の要求を、保存済み要求パラメータと完全比較した。
- 元の応答を変更しない再生では2ページ目も満杯なので、終端とは扱わない。
- syntheticの空終端を3ページ目へ追加するとpages_read=3、4件、next_before=Noneで停止。
- syntheticの短ページでは3件・2ページで停止。
- ページ境界の一致する重複は1件だけ除去し、新しい有効TXを残す。
  矛盾する重複とcursor循環は既存実装が拒否する。
- これらは既存Monitorの動作確認。実ネットワークで重複や循環が発生した証明ではない。

### 承認後の追加RPC：限定区間の終端を確認

保存データに終端応答がなかったため、実行メソッド・接続先・上限・対象・判定条件を提示し、
制限外実行の承認後に実施した。接続先はhttps://testnet.rialo.io/のみ。
getSignaturesForAddressのみ最大3要求・各10秒・再試行なし。
既存history_pagesを直接使用。実際は2要求で終了し、3要求目は実行していない。

対象address：`5grwhWabCAiyc7TXc3URVsRssV1sNskfQvJL3yBxGMNF`。
保存済みheight 2821131の署名をbefore、2821128の署名をuntilに固定しlimit=2。

1. 最初の応答はheight 2821130/2821129の2件。保存応答の同一Signature/heightと一致。
2. 最後のSignatureを次のbeforeへ渡し、untilは同じ値を維持。
3. 2ページ目がvalue=[]。pages_read=2、entries=2、duplicates_removed=0、next_before=null。

実行UTC：2026-10-25T06:49:42Z〜06:49:44Z。
context.slotは2830196/2830197、api_versionは両方0.23.0-alpha.0。
今回の範囲ではbefore/untilの境界署名を含まず、その間の2件を取得して終端となった。
これを全ノード・全時点の恒久仕様と一般化しない。

**PASSは指定した有限区間の終端処理に限定する。** untilで区切らない全履歴の終端、
全期間の完全性、保持期間、pruningの有無は今回証明していない。
この限定確認に追加のRPCは不要。

## 保持範囲・旧TX検索不能の原因

公式配布ソースはCargo registry配下の以下を参照（ユーザー固有の絶対パスは省略）。

| ソース | 確認結果 |
|---|---|
| rialo-cdk-0.18.1/docs/rpc-reference.md:242,263 | recentとlimit/beforeの説明はあるが、日数・最古高・archive保証は確認できない |
| rialo-api-types-0.18.1/src/messages/get_signature_statuses.rs:23,33 | search_transaction_historyはSolana互換用の未使用パラメータとの記載 |
| rialo-cdk-0.18.1/src/rpc/client.rs:619 | 状態検索config=None。slotをcontext.slotへ変換、executedは元の値を保持 |
| rialo-api-types-0.18.1/src/messages/get_transaction.rs:124付近 | 詳細検索はversion/signature。アドレス・時間条件・archive切替を指定するフィールドなし |
| rialo-api-types-0.18.1/src/messages/get_signatures_for_address.rs:32 | limit/before/until。検索可能日数の契約ではない |

保持仕様が見つからないことを「保持制限なし」「非対応」「削除あり」の証明にしない。
serverのledger/pruning/archive設定はクライアントソースでは分からない。
未使用と説明されたsearch_transaction_historyをtrueにするだけで、古いTXを検索できるとは主張しない。

過去3件のSignatureをSDK Signature::from_strで復号・文字列round-tripできた。
これはbase58/64byte識別子としての構文有効性のみであり、暗号署名の正当性、現在のchainでの存在、
永続保持を証明しない。既存記録の2026-10-11実行成功と10月22日Not Foundは別の観測として維持。

過去・今回とも設定/記録上のendpoint名はhttps://testnet.rialo.io/で、Devnetへ切り替えた証拠はない。
同一URLでも同一genesis/同一ledgerが継続している証明にはならない。
過去の31319418と今回2821117等の高さの差、api_versionの一致だけでリセットと断定しない。
保持範囲外・pruning・再配備・別ledger等は依然として仮説。

**保持期間、ネットワーク継続性、旧TX検索不能の根因はUNKNOWN。**
同じ古いTXを再照会してNot Foundを重ねても根因の識別が進まないため、今回再照会していない。
必要なのは公式運営のarchive/pruning契約、当時と現在のledger識別情報、旧TXに対する保存状態の証拠。

## SDK互換性

- getTransaction：保存済み5件を公式GetTransactionResponseと既存Monitorで再生成功。
  さらにSDK get_transactionが行うTransactionData/TransactionStatusMetadataへの応答変換を
  同じ5件に適用して成功。HTTPメソッド自体の追加live実行とは区別する。
- getSignatureStatuses：保存されているのはSDK変換後のSignatureStatus5件。
  executed=true/err=null、Succeeded/ExecutedWithoutErrorを既存observe_statusで再現。
  SDKは元の個別slotをcontext.slotへ置換するため、元wire payloadのslotを復元できない。
  元のGetSignatureStatusesResponseの完全なwire形はUNKNOWNのまま。
- getSignaturesForAddress：Monitorは公式context/value型を直接復号し、保存済み非空2ページと
  今回の限定区間live 2ページの両方で成功。
- **SDK便利メソッドの形式不適合を再現**：client.rs:727は戻り値Vec<generated::SignatureInfo>へ
  直接復号する。保存済みcontext/value全体はその型へ復号失敗し、内側valueだけなら復号成功。
  これは当該便利メソッドの外側応答形式についての互換性FAIL。
  既存Monitorはこのメソッドを使用せず、公式API型経由で回避しているので回帰失敗ではない。
- API版実測0.23.0-alpha.0とCDK0.18.1の番号差のみで互換性の有無を断定しない。
  上記で検査した使用経路についてのみ結果を付ける。

SDK便利メソッドを直すために依存SDKを差し替えたり、Monitorに不要なfallbackを足したりはしない。
既存の回避経路を維持し、必要なら後続で上流SDKへの不整合報告を検討する（今回は送信しない）。

## 判定チェックリスト（今回の定義）

| ID | 項目 | 判定 | 証拠範囲 |
|---|---|---|---|
| P01 | 履歴context/value復号 | PASS | 保存応答再生・限定live |
| P02 | cursor抽出・引き継ぎ | PASS | 保存要求照合・限定live |
| P03 | 2ページ目の取得 | PASS | 保存応答再生・限定live |
| P04 | 境界重複の除去と有効TX保持 | PASS | syntheticのみ |
| P05 | 矛盾重複・cursor循環拒否 | PASS | syntheticのみ |
| P06 | 空/短ページ終端のクライアント処理 | PASS | synthetic、空ページは限定liveも確認 |
| P07 | before/until指定の有限区間終端 | PASS | 今回live 2要求 |
| P08 | 詳細5件・SDKの追加応答型変換 | PASS | 保存live応答のオフライン再生 |
| P09 | 5件のexecuted/err/status_reason | PASS | SDK変換後の保存応答を再生 |
| P10 | API版と応答contextの保持 | PASS | 保存応答・限定live、原子snapshotは主張しない |
| P11 | 過去Signature3件の構文有効性 | PASS | SDK型round-tripのみ |
| F01 | SDK履歴便利メソッドの外側応答形式 | FAIL | 保存された実応答で不適合を再現。Monitorは回避済み |
| U01 | 全期間保持範囲・検索可能期間 | UNKNOWN | サーバー契約/設定なし、全履歴終端未確認 |
| U02 | 古いTXがNot Foundだった根因 | UNKNOWN | 識別可能な証拠不足 |
| U03 | 過去と現在の同一ledger継続性 | UNKNOWN | endpoint名/高さ差では確定不可 |
| U04 | 状態APIの元wire payload全体 | UNKNOWN | SDK変換後データしか保存されていない |

**PASS11 / FAIL1 / UNKNOWN4（16項目）**。
11/16=68.75%はこのチェックリストの単純なPASS比率に限る。
Phase 7Z全体の重み付き完了条件は定義されておらず、正式な総合達成率として扱わない。
3目的では、ページングは有限区間までPASS、保持範囲/旧TX根因はUNKNOWN、SDK互換性は部分確認。
総合PARTIAL PASS。失敗1は既知のSDK便利メソッド不適合で、既存Monitor/71件の破損ではない。

## 回帰と追加チェックの集計

既存189件：71実行・71成功・0失敗・118件D除外・安全対象の未実行0。
今回も既存Runnerのseccomp遮断を確認し、個別出力と件数の一致を確認した。
追加の独立したオフライン診断：11チェック成功・0失敗。
「SDK不適合を期待どおり再現した」チェックの成功と、SDK互換性判定のFAILを混同しない。
既存189件へこれらのチェックを加えてテスト数を改称しない。再実行も重複加算しない。

本番src/tests/examples/Cargo.toml/Cargo.lock/allowlistは無変更。
追加コードはscripts配下の再生診断のみで、71件の自動allowlistには追加しない。
`cargo fmt --check`、診断ソースの`rustfmt --check --edition 2024`、`git diff --check`は成功。
一時診断のコンパイルと実行は既存のネットワーク遮断下で行った。
追加RPC用の一時バイナリは別プロセスで、指定承認されたhistoryメソッド以外を拒否した。

## 残作業・見積り

- 全期間保持・旧TX原因：公式運営による保持契約/ledger識別の公開証拠が必要。
  ローカルRPCだけで完全な保持期間を推定しない。証拠があれば照合・記録1〜2時間、
  外部回答の待ち時間は見積不能。今回運営へのメッセージ送信はしない。
- 元wire statusの照合が必要なら、別途承認後にgetSignatureStatusesを公開既存5件へ一括1要求。
  endpointは同じhttps://testnet.rialo.io/、10秒上限・再試行なし。
  元API型の復号・個別slotとSDK変換の比較で判定。準備/照合は0.5時間程度。
  今回は必要性を記録するだけで、この追加要求は実行しない。
- SDK便利メソッドの修正は範囲外。上流仕様/修正案の整理なら0.5〜1時間程度。
  依存更新には別の安全監査・回帰検証が必要で、現時点では見積りに含めない。
- 全履歴の無制限追跡は行わない。保持期間の保証がなければ無限に近い照会でも完全性を証明できない。

新規TX作成、署名/送信、Faucet、秘密鍵/Seed/認証情報取得、AGP監視の操作は行っていない。
