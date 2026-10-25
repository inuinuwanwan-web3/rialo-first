# Phase 7Z — Testnet compatibility investigation

## GitHub記録時の更新

総合判定：**PARTIAL PASS（部分成功・未完了）**。
後続の承認済み検証で公開TX5件を発見し、5件とも詳細取得、executed=true/err=null、
Succeeded/ExecutedWithoutErrorを確認した。オフライン回帰は71成功・0失敗・118除外。
SDK互換性は使用したMonitor経路のみ確認。非空2ページは取得できたが終端までの検証は未完了。
古いTXの検索不能の根因は未確定。詳細は[実Testnet検証](phase-7z-live-testnet-validation.md)と
[実測・回帰証拠JSON](phase-7z-live-testnet-results.json)を参照。

以下は外部照会承認前の調査段階の記録。承認待ち・live照会0件という記述は当時の状態であり、
現在の到達状況は上記と後続の実Testnet検証を優先する。

2026-10-25。基準HEAD: 7d643df3660c0789c490569a21b73e486cf1e0d3。
Phase 7Zを維持。外部照会は別途承認待ち。新規TX/署名/Faucetでデータを作らない。
既存未追跡docs/phase-7-signing-investigation.mdには触れない。

## ローカルで確認済み

既存Phase7Y記録・Phase7Z実装/監査とインストール済み公式SDKソースを照合した。
SDKはCargo.toml/Cargo.lock固定のrialo-cdk 0.18.1。
ネットワーク設定はsrc/lib.rs:10のhttps://testnet.rialo.io/。
GitHub保存済みHEADとローカルHEADは上記。今回GitHubへの外部再照会はまだ行わない。

ソース基点：導入済みCargo registryのソースディレクトリ（ユーザー固有パスは省略）。

| 対象 | ソース | 確認事項 |
|---|---|---|
| getTransaction | rialo-cdk-0.18.1/src/rpc/client.rs:586 | 署名をversion=0のrequestへ。公式GetTransactionResponseを復号後にCDK互換型へ変換 |
| 詳細API型 | rialo-api-types-0.18.1/src/messages/get_transaction.rs:12,34,85 | context/block_height/block_time/transaction/meta。errはOption<Value>。null詳細を正常型としない |
| getSignatureStatuses | rialo-cdk-0.18.1/src/rpc/client.rs:619 | response.valueを全件Some化。slotをcontext.slotへ置換しerrを文字列化 |
| 状態API型 | rialo-api-types-0.18.1/src/messages/get_signature_statuses.rs:41,60 | context/value、各slot/executed/err。valueはOption要素ではない |
| getSignaturesForAddress | rialo-cdk-0.18.1/src/rpc/client.rs:727 | Vec<SignatureInfo>へ直接復号する便利メソッド |
| 履歴API型 | rialo-api-types-0.18.1/src/messages/get_signatures_for_address.rs:32,71,88 | context/value、before/untilはSignature文字列、limit1..1000 |
| Monitorの回避経路 | src/monitor/query.rs | call_with_jsonから公式context/value型を直接復号し、SDK便利メソッドの形式差を回避 |
| 公開候補探索API | rialo-cdk-0.18.1/src/rpc/client.rs:813、rialo-api-types-0.18.1/src/messages/get_transactions.rs:18,32,81 | getTransactions、limit/before、context/value。SDK変換でblock_height等を落とすため、必要時は元API型/応答を保持 |

getSignatureStatusesのslot変換はTX包含高/確認数の計算に利用できない理由だが、
SDKがexecuted=falseを新たに作る処理ではない。Unknown判定は既存Monitorを維持する。
履歴便利メソッドの型の差は既知で、既存Monitorで対処済み。新たな修正は不要。

## 既存記録で確認済みの実測（今回の現在値ではない）

- docs/2026-10-11-testnet-transfer-e2e.mdには、公開TX3件の当時の成功記録がある。
- docs/phase-7y-history-status-investigation.mdには10月22日の詳細Not Found、
  executed=false/err=null、2アドレスの空履歴、api_version=0.23.0-alpha.0の記録がある。
- 当時DNS/TLS接続は成功し、詳細はアプリケーション層のRPCエラーだった。
- 当時の公開TX候補は3件とも現在参照できると証明されていない。
  synthetic fixtureを実TX候補へ混ぜない。

## 原因の分類

| 区分 | 判定 | 根拠・次の識別方法 |
|---|---|---|
| 安全なオフライン経路の実装不足 | 否定できる範囲あり | Phase7Zで詳細・非空履歴・ページング・状態をsynthetic型付きfixtureで検証済み。ただしlive互換性は別 |
| 詳細/非空履歴のlive証拠不足 | 確認済みの未完了理由 | 現在参照可能なTXと非空履歴を取得した記録がない |
| SDK履歴便利メソッドの型不整合 | ソース確認済み | Monitorは回避済みなので、既存MonitorのNot Foundの原因とは認定しない |
| SDKとサーバーの全面的非互換 | 未確認 | CDK0.18.1とAPI0.23.0-alpha.0の版番号差だけでは判定不可。空履歴/状態は過去に復号成功 |
| 履歴保持期間/pruning/archive制限 | 仮説 | 保持範囲の公式保証/現在のノード設定が未確認 |
| Testnetリセット/別chain/旧環境 | 仮説 | 昔と今の高さの差だけでは証明不可。ネットワーク識別と公式告知が必要 |
| 過去TXの実行失敗/未存在 | 認定しない | 現在の検索不能は過去成功の記録を否定しない |
| 現在の接続/HTTP障害 | 未検証 | 今回は外部照会未実施。10月22日の接続成功は現在の保証ではない |

公式公開情報の最新確認、現在の公開TX探索、保持範囲の公式記述の確認は承認後。
公開情報に仕様がないことを、機能未対応/リセットの証明にしない。

## 外部照会の提案（承認前は実行しない）

1. 公式Rialo公開docs/Explorerを最大10ページ、RPC仕様・endpoint・公開TX・保持範囲に限定して読む。
2. https://testnet.rialo.io/のみ、JSON-RPC読み取り最大12要求、各10秒、再試行なし。
3. getBlockHeightで到達性と現在高、getTransactions(limit=2)で直近最大2件を探索。
4. 一覧で得た公開TXまたは既存公開記録のTXにgetTransaction/getSignatureStatuses。
5. 取得詳細の公開accountKeysまたは既存公開記録のaddressについて、
   getSignaturesForAddress(limit=2、最大2ページ)を照会。cursorがなければ追加ページなし。
6. 要求時刻/パラメータ、実際の応答形、RPCコード/メッセージを記録。
   詳細取得成功、実行成功、履歴非空、pagination成功を別々に評価する。
7. 空/Not Foundの場合はその事実だけを記録。探索の無制限拡張や新規TX作成をしない。

外部照会の承認はユーザーへ提示済み。承認なしにweb検索やRPCを行わない。

## 実装方針・残作業

現時点では本番Monitor/既存テスト/許可リストに変更しない。
既存Runnerで同じ71件を通信遮断下で回帰実行する。
外部応答を得た場合のみ、必要な最小限の読み取り診断追加を判断する。

所要時間の目安（作業時間、待ち時間を除く）：
承認された公開情報確認と限定RPC照会・分析・記録は1〜2時間。
参照可能TXが得られれば追加の詳細/履歴/ページング照合は0.5〜1時間。
仕様差の最小修正が必要な場合は別途2〜4時間程度で見積り直す。
公開TXやarchive保持の提供を外部へ依存する場合、完了までの暦時間は推定不能。

commit/push、Notion、AGP操作、秘密情報取得、TX生成/署名/送信、Faucetは行わない。

## 今回のオフライン回帰結果

実行UTC：2026-10-25T06:14:55.408285+00:00 ～ 2026-10-25T06:16:00.987299+00:00。
既存RunnerのLinux seccomp遮断を確認して189件中71件実行・71成功・0失敗・118件D除外。
当段階の個別結果は記録保存前に照合済み。後続回帰の個別証拠は保存対象JSONへ収録。
既存Monitor、テスト定義、許可リスト、過去の検証記録は変更なし。
今回のlive照会は0件。根本原因の判定は未完了で、外部照会承認待ち。
安全な限定照会計画の準備とローカル互換性整理は完了。
