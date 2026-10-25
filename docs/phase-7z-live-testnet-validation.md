# Phase 7Z — Live Testnet Monitor validation

2026-10-25。基準HEAD 7d643df。**総合判定PARTIAL PASS（部分成功・未完了）**。
今回の指定7項目はPASSだが、履歴終端・Explorer直接照合・旧TX検索不能の根因などを残し、
Phase 7Z全体の完全成功とはしない。
本番コード/既存189件/許可リストを変更せず、既存SDKとMonitorを再利用した。
検証時はGitHub/Notionへの記録・commit/pushを行わず、本記録保存時のみ指定3ファイルをGitHubへ保存する。Notion操作なし。

## 情報源・信頼性

- 対象Explorer https://testnet.rialoscan.org/ と /txs はWeb取得ツールでは参照不能。
  Explorer自体の停止やネットワーク停止を証明する結果ではない。
- https://github.com/Orvynel/rialoscan は自身をindependent explorerと説明し、
  READMEからtestnet.rialoscan.orgへリンクする。公開リポジトリの管理主体はOrvynel。
  Rialo/Subzero Labs公式運営という証拠は得られず、法的な運営主体・現行配備との同一性は未確認。
- 同READMEはRPC直接読み取り・indexerなしと説明するが、これは開発者の説明。
  現行Explorer画面の個々の数値は今回直接照合していない。
- READMEの古いlimit/before無視という説明は現行仕様の証明ではない。
  今回の公式RPCではlimit=5の5件feedとlimit=2の2件pageを実測し、beforeで古い履歴へ進んだ。
  TypeScript SDKの不具合説明をRust CDKの不具合へ転用しない。
- https://docs.rialo.io/ はCloudflare Accessログインへ転送されたため、認証や回避をせず停止。
  公式SDK0.18.1の仕様は前段の導入済み公式配布ソースを使用した。

Explorerの掲載情報をTXの実在証拠として採用せず、今回の5件は
https://testnet.rialo.io/ のgetTransactions応答から採用し、同じ公式RPCで詳細を再取得した。
単一endpointでの整合確認であり、独立ノード間の合意/Finality証明ではない。

## 実行範囲・安全性

外部RPCについて内容を提示し、制限外実行の承認後に1回実行。
環境はPATH/LANGのみ、接続先固定https://testnet.rialo.io/。
最大12要求、各10秒、再試行なし、全体125秒/外側130秒。
実行開始UTC `2026-10-25T06:27:03+00:00`、終了UTC `2026-10-25T06:27:07+00:00`。

実際は10要求：getBlockHeight×1、getTransactions(limit=5)×1、getTransaction×5、
getSignatureStatuses（5署名一括）×1、getSignaturesForAddress(limit=2)×2。
一覧から詳細検証する候補は最大5件。履歴応答中の追加4署名はページ検証用に観測しただけで、
追加のTX詳細探索対象にはしていない。探索の無制限拡張なし。

診断ソースと実行バイナリは
一時領域だけに作成。SDKの既存rlibとリンクし、新依存/新Cargo target/本番変更なし。
初回一時コンパイルのrlib選択不一致は修正し、SDKと同一featureのserde_jsonへリンクした。
これは診断ビルドの問題で、RPC互換性や既存テストの失敗ではない。

詳細はmonitor::query::transaction_detail、状態はMonitorRpc::statuses→observe_status、
履歴はmonitor::query::history_pageをそのまま使用。新たな状態判定器は作っていない。
公開候補の探索だけはSDK call_with_json(getTransactions)→公式GetTransactionsResponseで復号。
署名フィールドは既存公開TXの識別子として読むだけで、暗号署名を生成しない。
TX作成/署名/送信/Faucet/秘密情報取得/wallet接続/AGP操作なし。

## 公開TX 5件

取得元は全件 https://testnet.rialo.io/ のgetTransactions。
一覧取得UTC `2026-10-25T06:27:04+00:00`。
以下Block Heightは一覧と詳細で一致。全件詳細の先頭Signature一致もMonitorが確認した。

| TX Signature | Block Height | executed / err | status_reason | 詳細再取得UTC |
|---|---:|---|---|---|
| `5cv38KgppnNwyeiFb9Hc2JAQb4P6zbhYtU9i21wyvNmxukmG4U57j9pYCJWtiwfafkR4c9qhZN4TCrfxB279cvt7` | 2821117 | true / null | ExecutedWithoutError | 2026-10-25T06:27:05+00:00 |
| `4cSXa4scn5vRztEmBmDCzVXjvDiQaXFLBN1qiUxcDTxJa5P5xGX48DzqXS2j6DLtEHPZts2PvYXS4ueGmGDEzgoZ` | 2821116 | true / null | ExecutedWithoutError | 2026-10-25T06:27:05+00:00 |
| `2LhuzDiCpU8W9TxXx8z4UfVWSScp5DrR5Zbaf68tKAecEKvkZpUvsXyieGmP9Fnfj2ZGQ2svHdjkCTaiDTqLXN8P` | 2821115 | true / null | ExecutedWithoutError | 2026-10-25T06:27:05+00:00 |
| `wmJpjL7SYnpBbKUsyvcXwZwQAA5XJ89ACps9fQ2gVCswC1au6dSTFYNiZfC9PFPFRp7bke2gcGNdEJkPpBSEsV9` | 2821114 | true / null | ExecutedWithoutError | 2026-10-25T06:27:05+00:00 |
| `VFJJCZhnoNDPXtjtTnMVVbgHm7TPfiHgevWLWTmxhSPNPKxessotr6xQZhskyhFrjGCmMLbN7Gm7vhfCp8A65eP` | 2821113 | true / null | ExecutedWithoutError | 2026-10-25T06:27:06+00:00 |

getBlockHeightの実測は2821115。その後の別要求でより高いTXが返ったのは別時点の観測であり、
原子的なsnapshotや矛盾とは扱わない。
SDK変換後status.slotは全件2821129だがcontext由来で、TX包含高ではない。
詳細block_heightを包含高の観測として記録し、確認数/Finalityへ変換しない。

## 非空履歴・ページング

対象公開address（取得した詳細のaccountKeysから選択）：`5grwhWabCAiyc7TXc3URVsRssV1sNskfQvJL3yBxGMNF`。
1ページ目heightは[2821131, 2821130]。
その最後の署名を2ページ目のconfig.beforeへ渡し、要求記録と一致することを確認。
2ページ目heightは[2821129, 2821128]。
2ページ合計4件、重複0。両ページでcontext/value形式の公式API型復号成功。
2ページ目も満杯でnext_beforeあり。3ページ目は予算/範囲に従い未実施。
従って履歴終端・全期間完全性・実重複発生時の除去はlive未検証。
今回の並びが降順だったという実測を、公式の恒久的な順序保証とはしない。
history_pages集約helperは今回liveでは直接呼ばず、ページ要求/復号の既存history_pageを使用した。
集約helperの重複/循環拒否は既存オフラインテストで検証を維持。

## 判定表

指定7項目：PASS7 / FAIL0 / UNKNOWN0。
残課題3項目を含む下表全体：**PASS7 / FAIL0 / UNKNOWN3**。

| 項目 | 判定 | 観測・限界 |
|---|---|---|
| getTransaction（Monitor経路） | PASS | 5/5の詳細再取得・Signature/height保持 |
| getSignatureStatuses（SDK実装） | PASS | 5署名がSDKで復号、全件状態取得 |
| getSignaturesForAddress（Monitor経路） | PASS | 公式context/value型で非空履歴復号 |
| executed / err | PASS | 全5件true/null、詳細meta.errもnull |
| status_reason | PASS | 全5件Succeeded / ExecutedWithoutError |
| 履歴取得・ページネーション（2ページの限定範囲） | PASS | 非空2ページ、before引き渡し、重複なし。終端までのページネーション検証は未完了 |
| SDKとRPCの応答形式（使用経路） | PASS | CDK0.18.1と現在のAPI応答で詳細/状態/履歴/一覧の型復号成功 |
| Explorer表示と公式RPCの直接照合 | UNKNOWN | WebツールでExplorer本体を取得できない |
| 履歴終端・全期間の保持範囲 | UNKNOWN | 2ページ目も次候補あり。archive保証なし |
| 旧公開TXが10月22日に見つからなかった根因 | UNKNOWN | 今回は直近TXを使用。保持制限/pruning/リセットの証拠なし |

## SDK互換性の適用範囲

今回のapi_version実測はgetTransactions/詳細/履歴とも0.23.0-alpha.0。
CDK版番号とノード版番号の差だけでは互換性を判断せず、実復号成功で上記経路の互換性を確認。
SDK get_signatures_for_address便利メソッドはVecへの直接復号で、現在のcontext/valueと
形式差があることをソース確認済み。Monitorは公式API型を直接使用して既に回避している。
この便利メソッドを今回別途live実行して成功したとは主張しない。
SDK get_transaction便利メソッドの追加変換も直接live検証対象ではなく、
Monitorの公式GetTransactionResponse復号までを今回のPASS範囲とする。
状態の元wire payloadはSDK変換後しか保存していないため、個別元slotは推測復元しない。
失敗TX/未実行TX/Not Foundケースのlive検証は追加せず、合成fixtureのテストを維持した。

## オフライン回帰との比較

既存Runnerで通信遮断を確認し、189件中71実行・71成功・0失敗・118件D除外。
検証時の実行JSONと個別ログを照合済み。保存対象JSONの`offline_regression`に
日時・71件の個別成功出力・118件の除外一覧を収録（不要なローカルパスは省略）。
本番ソース・189件の定義・許可リストに変更なし。fmt/diffチェックも成功。
synthetic成功/非空履歴/カーソル処理に加え、今回は実応答の成功5件/非空2ページを確認。
syntheticで成功した重複/循環/異常系すべてがliveでも観測されたとは主張しない。

## 証拠と残作業

[実応答・要求記録JSON](phase-7z-live-testnet-results.json)に10要求のSDK結果、
候補・詳細・状態・履歴・取得時刻を保存。API error/timeoutは今回0件。
HTTPヘッダー/HTTP全体の生envelopeは保存していない。

- Explorer画面との直接比較が必要なら、別の閲覧経路で取得し再照合：約0.5時間。
- 保持範囲・旧TX根因は公式archive/pruning/ネットワーク識別の情報確認：約1〜2時間の調査。
  公式情報提供待ちの暦時間は見積不能。
- 終端確認やlive失敗/重複ケースを追加する場合は、公開既存データと追加の少数RPC許可が必要。
  適切なデータがあれば約0.5〜1時間。新規TXを作って補わない。

次に優先するのは、今回到達した限定live成功を確認したうえで、
履歴完全性/旧TX保持のどこまでをPhase7Zの完了要件とするか整理すること。
完全な履歴保持や全SDKメソッドの互換性までPASSと拡張しない。
