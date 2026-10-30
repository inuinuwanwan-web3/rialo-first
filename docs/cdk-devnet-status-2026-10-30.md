# CDK再開：DevNetの元応答を保持する状態照会

開発時点のオフライン報告。後続の実DevNet検証はPASS。最終状態・数値は
[完了記録](cdk-devnet-status-completion-2026-10-30.md)を参照。

## 調査と選定

2026-10-30。開始ブランチmain、HEAD a5e8704ae884770ddc21bd98ce9ad2f3402abe57。
GitHub公開main参照も同じcommitであることを読み取り確認。
開始時のCargo.toml・src/monitor.rsの変更、未追跡Journal/診断/AGP記録を保持する。

| 候補 | 実装価値 | 目安 | 制約・リスク |
|---|---|---|---|
| 元の個別slot・型付きエラーを保持するDevNet状態照会 | 既存SDK変換で失われる証拠を保持し、公開TXの調査に利用できる | 1〜2時間 | ローカル公式型と実サーバーの互換性は別途確認が必要 |
| アカウント情報・owner等の読み取り | 将来のプログラム利用前の調査に有用 | 2〜3時間 | データencoding・出力範囲を追加設計する必要がある |
| Phase 8N実ENOSPC連携の続行 | 永続Journal運用の耐障害性を確認できる | 方式確定後3時間以上 | 管理者権限・容量試験・稼働監視との資源共有があり、今回の優先条件に適さない |

第1候補を選択。履歴取得・ページング・Journalを作り直さない。
新しいDevNet状態照会入口を追加し、既存observe_statusで状態を分類する。
実装・見積りの範囲は限定照会で、常駐化・確認数・finality認定は含まない。

## 既存の完了地点と未完了事項

- Phase 7X：src/monitor.rsのbalance/height/status snapshotと既存Mockテスト。
- Phase 7Y〜7Z：src/monitor/query.rsの公式型経由の詳細・履歴・有限ページング、
  tests/phase_7z_monitor.rs。7Z保存記録は既存71件＋独立4件成功。
  SDK履歴便利メソッドの既知不適合は既存経路で回避済み。品質改善は終了可、
  全履歴保持・旧TX検索不能原因・ledger継続性等はUNKNOWNのまま。
- Phase 8A/B：未commitのJournal、offline CLI、耐障害テストは実装済み。
  src/monitor/journal.rs、同tests.rs/resilience.rs、src/bin/phase8a_journal.rsを確認。
- Phase 8C以降：限定live記録、復旧・隔離・Guardian/lease等のコードと試験記録あり。
  全体の実運用資格化・実ENOSPC・5分/24時間試験は完了としない。
- GitHubのCDK関連記録は6e35bef（8N限定検証PARTIAL PASS）、最新HEADは
  a5e8704（AGP Phase 8P参照記録）。8PはCDK Journalの完了を意味しない。
- 署名・送信の未完了事項は今回対象外。既存CLI/ライブラリの署名・鍵テストを再実行しない。

## 公式仕様の根拠

固定依存の公式配布ソースを使用：

- rialo-api-types 0.18.1/src/messages/get_signature_statuses.rs：version=0、
  最大256署名、configはSolana互換用の未使用項目、context/value、各slot/err/executed。
- 同rpc_response_context.rs：context.slotと任意api_version。
- rialo-cdk 0.18.1/src/rpc/client.rs:619付近：同requestを使用する一方、
  SDK互換型への変換で個別status.slotをresponse.context.slotへ置き換える。
- rialo-cdk::constants::URL_DEVNET：CLI接続先を導入済み公式SDK定数に固定。

公開資料：[CDK 0.18.1](https://docs.rs/crate/rialo-cdk/0.18.1)、
[公式配布API型](https://docs.rs/crate/rialo-api-types/0.18.1/source/src/messages/get_signature_statuses.rs)。
オンラインのAPI型ページ取得は今回の閲覧ツールで失敗したため、型の詳細根拠は
導入済み公式配布ソースとする。最新サーバー仕様の完全一致は認定しない。

## 実装と使用方法

- src/status_readonly.rs：既存Monitorの分類を再利用し、公式レスポンス全体を型付きで保持。
  各TXのslotとcontext.slotを区別し、入力順で署名と結果を対応付ける。
  任意のRPCメソッドを渡せない固定getSignatureStatuses adapter。
- src/bin/devnet_status.rs：既定は入力検証だけ。--executeがある場合のみDevNetへ接続。
- src/lib.rs：新moduleの公開を1行追加。既存の変更ファイルを編集しない。
- tests/status_readonly.rs：合成データのみ。実鍵・署名操作・本番TX・AGPを使用しない。

ネットワークなしの入力確認：

```sh
cargo run --offline --locked --bin devnet_status -- --signature PUBLIC_BASE58_SIGNATURE
```

将来の明示的な読み取り実行は同じ引数に--executeを追加する。
これはライブRPCを発生させるため、今回は実行していない。
--signatureは複数指定可能。1〜256件、重複拒否、timeoutは1〜30秒、既定10秒。
要求は1回だけで、再試行・ポーリング・ファイル保存なし。
空/不正入力を通信前に拒否し、null・不正型・件数不一致・未知versionは正常報告にしない。
未実行はUnknownのまま扱い、Not Found・失敗・finalityを推測しない。
nullの個別statusは固定版公式型にないため、互換fallbackを発明せずエラーにする。

## 検証範囲

実行結果は同日のcdk-devnet-status-results-2026-10-30.jsonに記録する。
新規テストと影響範囲の既存Monitor回帰だけを実行する。
既存Phase 7Z/8A/8B runnerの監査manifestは過去snapshotに固定されているため、
書き換えて全件実行せず、ネットワーク遮断helperだけを再利用する。
コンパイル/テストには既存Linux seccompを適用し、IPv4/IPv6/Unix socket生成拒否を確認。
専用一時HOME/TMPDIR、cargo --offline --locked、ビルド並列2で実行する。

実測：新規9件、既存Monitor本体14件・query15件・CLI12件・7Z結合11件、
合計61成功・0失敗。別枠のCLI smoke3条件（help、既定の非接続入力確認、
書き込みoption拒否）も成功し、登録テスト61件へ水増し加算していない。
新規Rustファイルのrustfmt --check、git diff --checkも成功。
既存依存proc-macro-error2 2.0.1の将来Rust互換warningは出たが、今回の失敗ではなく未修正。
開始時の既存ファイルで変わったのはsrc/lib.rsへのmodule公開1行だけ。
Cargo.toml、Cargo.lock、src/monitor.rsと既存の未追跡成果物は開始時hashと一致。
AGPコード・設定・serviceを含む68ファイルも開始時hashと一致。
ネットワーク遮断helperは全コンパイル・テスト・CLI smokeの子processに適用した。
旧manifestに自動追加しておらず、61件は今回選択した影響範囲の回帰であり、全suite成功の主張ではない。

## 残る境界

DevNetでの実応答、最新サーバーのschema互換、元wire statusのlive確認は未検証。
本機能のオフライン成功でPhase 7ZのU04を解消済みにしない。
取得対象TXの実在・署名の暗号学的正当性・archive保持・ledger継続性を証明しない。
次に必要なのは、必要性が生じた場合の公開済みDevNet署名に対する1回の限定読み取り。
今回追加RPC、AGP操作、Telegram送信、署名・送金・秘密鍵アクセス、commit/push、
Notion記録は実施しない。Phase 8Nの容量試験・管理者操作は再開しない。
