# 2026-11-01 共通READ ONLYレポート オフライン集計・比較

**オフライン開発PASS。RPC・HTTP通信、GitHub/Notion記録は実施しない。**

## 既存実装と設計
`src/readonly_report.rs` のSCHEMA_VERSION=1.0とMAX_JSON_BYTES=1MiBを再利用する。既存Reportは出力用Serialize型であり、入力側には検証・許可項目投影専用処理を追加した。既存監視・履歴処理は照会と出力アダプターであり、複数保存レポートのオフライン集計・時系列比較機能はなかった。
新しい `report_analysis` バイナリと専用モジュールを追加。既存CLI・ライブラリ公開API・SDK・HTTP Transport・Cargoファイルは一切変更しない。

## 使用方法
```sh
cargo run --offline --locked --bin report_analysis -- report-a.json report-b.json
cargo run --offline --locked --bin report_analysis -- --json report-a.json report-b.json
```
ビルド済みなら `target/debug/report_analysis [--json] REPORT.json ...`。
明示した公開レポートのみ入力する。ディレクトリ探索、wallet/config/environmentの自動読み取りはない。秘密情報のファイルを入力しないこと。エラー時も固定理由のJSONを標準出力へ1個出し、exit=2。正常時exit=0。入力は変更しない。

## 集計仕様
総数は重複除外後のレポート数。input_filesは指定ファイル数、duplicates_skippedを別記する。
重複はJSONオブジェクトキー順と空白を正規化した完全一致のSHA-256で除外。日時・診断等が異なれば別レポート。同一対象・時刻でも内容が異なる場合は重複扱いにしない。
success、not_found、errors（rpc_error/http_error/timeout合計）、blocked、unverified、status別・network別・operation別件数と最初/最後のUTC観測日時を出す。not_foundは成功件数へ混ぜない。
入力日時は生成レポート仕様のUTCミリ秒固定形式、実在するGregorian日付（1970〜9999年）のみ受理する。観測時刻はレポート作成日時でありblock時刻とはしない。

## 比較仕様
network・operation・SDK構文検証済みaddress/signatureでグループ化。異なるnetwork/operation間は比較しない。時間昇順の隣接観測をbefore/afterとchangedで表示し、値が存在しない項目を補わない。差額・進行・finality等は推測しない。
- balance_status_snapshot：公開address、balance_kelvin。
- account_lookup：公開address、存在/不存在、owner、executable、balance_kelvin、rent_epoch、space、Dataの数値要約のみ。
- transaction_status：各signatureのslot/executed/error_present/execution。
- transaction_detail：signature、block_height/error_present。
- 全対象：保持されたcontext.slot（nullはnullのまま）。
履歴には照会元addressが既存形式に含まれないため、履歴スナップショットを同一対象と推測しない。対象識別子不足・エラー/blocked/unverifiedは比較対象にせずincomparable_reportsで明記する。単一観測はsingle_observation。同一時刻が複数ある対象では、曖昧な時刻を含む隣接比較をsame_timestampとして拒否する。
比較のstatusは照会状態、executionはTx自体の状態として区別する。テキスト形式は集計見出しと対象ごとのbefore/after、JSON形式はanalysis_schema_version=1.0の集計と比較配列。

## 安全制限
- 入力最大64ファイル、各1MiB、合計8MiB。重複も合計受信量に算入。
- ファイルを1件ずつ上限+1バイトまで読み、超過時拒否。全ファイル本文を一括保持しない。
- .jsonの通常ファイルのみ。symlinkと親symlinkを拒否し、wallet/secret/credential/token/keypair/.aws/.ssh/.envを含むパスを開く前に拒否。これだけで任意の入力内容が秘密でないと保証するものではない。公開レポートのみを明示入力する。
- JSON解析前に文字列内の括弧を除外してネスト上限32を検査。serde_json既定の再帰制限も維持。
- 数値はi64/u64範囲の整数のみ。float・指数overflow・巨大整数を拒否。slot・残高等の投影値はu64、boolはboolとして検証。
- Txエントリー256件/レポート、比較点合計4096。上限超過は切り捨てずエラー。
- JSON出力は制限付きwriterで1MiB、テキストも追記前に1MiBを検査（末尾改行1バイトは別）。失敗時に部分出力しない。
- Data生バイナリ、自由なerror/provider本文、未知の文字列フィールドは出力しない。公開識別子・固定列挙・許可数値/真偽値だけを投影。

HTTP本文上限ではなくローカル入力/出力上限。JSON Valueの構造・比較用のコピー・出力作成に追加メモリを使うため、プロセスRSS/CPUの厳密上限は保証しない。同時に悪意を持ってファイル/親ディレクトリを差し替える環境への競合安全性は未検証。所有者が管理する静的レポートを利用する。

## 検証
初回は新規json!式の構文ビルドエラーを修正（テスト未実行）。その後、新規25件＋既存CLI5件成功。レビューで投影型・Unconfirmed・同一時刻の曖昧性・出力制限を補強し、新規全31件を再実行、31成功0失敗。
**一意件数：新規31＋既存5＝36件成功、失敗0件。延べ61実行。**
通常/複数入力、全status/network/operation集計、時系列、残高・Tx・account比較、比較不可、実CLI両形式、不正/空JSON、schema/必須欠損、日付、重複、巨大整数、深さ、ファイル数/各サイズ/合計、read失敗、秘密パス、symlink、出力制限を検証。
`cargo test --offline --locked` を既存Linux seccompネットワーク拒否下で実行。既存CLI互換5件はネットワークを使わない既定/JSON/dry-run/error経路。新CLIにもRPC/HTTPクライアント構築なし。
開始時hashとの照合により既存src/tests/docs/Cargo全ファイルの変更なし。新規コード3件＋本書/結果JSONの計5件のみ追加。正確な件数とソース/ログhashは `cdk-offline-report-analysis-results-2026-11-01.json`。

## 未検証・引き継ぎ
実データを使った利用者受入、最悪ケースのRSS/CPU測定は未実施。既存のRPC/HTTP Transport/AGP監視は変更・再試験していない。新CLIは完全オフラインでありDevNetオンライン検証は不要。記録公開は次の指示を待つ。

## GitHub公開前の最終確認（2026-11-01）
既存main 6ad67b2から分離した公開用作業コピーで、今回の5ファイルだけを追加した。追加のモジュール登録・Cargo変更は不要。rustfmt --check、git diff --cached --check成功。公開用ソースと既存検証記録のhash一致、秘密情報・個人情報の混入なしを確認した。
新規31件＋既存CLI互換5件をネットワーク拒否下で実行し、36件成功・失敗0件。JSON両出力、入力制限、エラー処理を含む。cargo build --offline --locked --bin report_analysis成功。無関係な全テストは、既存コード変更がないため再実行しなかった。既存依存proc-macro-error2の将来互換性警告は残るが、ビルド・テストは成功した。
最終確認は開発時の延べ61実行とは別の36実行。Notionは利用者により記録済みのため今回操作しない。元の作業ツリーの無関係な変更を保持し、通常push後にリモートmainとの一致を確認する。
