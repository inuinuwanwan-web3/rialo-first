# 2026-11-01 READ ONLY JSONレポート共通化

**PASS：選定した既存READ ONLY機能のオフライン出力共通化。新形式のオンライン検証は未実施。**
RPC機能・署名・送金を追加せず、既存データを出力用の安全な要約へ変換する。

## 既存実装の確認と範囲

開始時はmain、ローカルHEAD a5e8704。分離公開で記録済みの成果を含むローカル未コミット変更を保持した。
今回GitHubへの照会・commit/push、Notion記録は行わない。

| 根拠 | 既存処理と今回の扱い |
|---|---|
| src/monitor/cli.rs（開始時:73 execute） | Testnetの残高・高さ・Tx状態JSON。共通のschema/time/error外枠はなかった |
| src/bin/devnet_status.rs（開始時:29以降） | DevNet状態の独自JSONと固定エラー文字列。--jsonで新形式を明示選択 |
| src/bin/devnet_account.rs（開始時:38以降） | SDK/独立Transport共通のAccountReport、独立経路のHttpReceiptを再利用 |
| src/monitor/query.rs:43/52/173 | HistoryPage/HistoryBatch/Tx詳細の既存型。新たなRPCやCLIを作らず出力adapterを提供 |
| src/status_readonly.rs | 個別slotとcontext.slot保持。分類処理を再利用。timeoutだけ同じ表示文の型付きエラーへ変更 |
| src/monitor.rs:83 observe_status | Tx実行結果の既存分類を再利用 |

既存DTOと分類は再利用するが、全機能に共通する外枠・UTC・出力量制限はなかったため追加した。
残高は既存Testnet Monitorのsnapshotに含める。独立の新しいbalance RPCは追加しない。
履歴/詳細はライブラリadapterまでで、既存のphase_7y_readonly診断exampleの出力は変更しない。
従来の人間向けrpc-check/testnet-readonly/準備モードと、保存処理を行うJournal CLIは今回の対象外。

## 共通仕様1.0

| フィールド | 意味 |
|---|---|
| schema_version | 固定文字列1.0。サーバーAPI version/JSON-RPC versionとは別 |
| network | devnetまたはtestnet。設定された対象ラベルで、chain真正性の証明ではない |
| operation | balance_status_snapshot / transaction_status / transaction_history / transaction_detail / account_lookup |
| timestamp_utc | レポート生成時のUTC、YYYY-MM-DDTHH:MM:SS.sssZ。block時刻ではない |
| status | success / not_found / rpc_error / http_error / timeout / blocked / unverified |
| context.slot | 実際に型から取得できたslot。ない場合はnull。block_heightをslotへ代入しない |
| data | 許可された公開項目の要約。エラー時null |
| error | 固定分類・固定理由・数値code/http_statusのみ。正常時null |
| transport | sdk / bounded_http / provided_data。失敗fallbackで不明の場合はnull |
| http | 独立Transportの検証済み数値診断。ない場合はnull |
| query_attempted | CLIで照会処理に進んだ場合true、未実行/提供データ変換のみならfalse。wire要求回数の計測ではない。不明fallbackはnull |
| verification | 常にnot_asserted。照会結果を開発試験のPASSやオンライン実証と混同しない |

successは照会/変換が成功した意味であり、Txが成功/完走/finalizedした意味ではない。
TxのFailed/Unknown等はdata内のexecution/status_reasonへ残す。
not_foundは明示的なアカウント不存在で、通信失敗ではない。HTTP 200診断と併存する。
空履歴・未実行Tx・検索不明をnot_foundへ推測変換しない。
--executeなしの入力検証はunverified、query_attempted=falseとし、DevNet PASSとはしない。
ライブラリのhistory/detailはprovided_data。monitor/status/account adapterも入力の由来を明示する。

UTC生成は標準ライブラリとGregorian変換で実装し、依存追加なし。1970〜9999年を受け付ける。
不正時刻はエラー。出力不能時の固定blockedレポートで時計自体が不正ならtimestamp_utc=nullとし、偽時刻を補わない。

## 接続と互換性

```sh
# 既定のJSON/人間向けモードは維持。共通形式は--jsonだけで選択する。
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY --json
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY --bounded-http --json
cargo run --offline --locked --bin devnet_status -- --signature PUBLIC_SIGNATURE --json
```

上記は--executeがないため接続しない。Testnet Monitorは従来から実行するモードであるため、
次の書式はライブ照会を行う。今回は実行していない。

```sh
cargo run --offline --locked -- --testnet-monitor --address PUBLIC_KEY --json
```

--jsonを付けない既存JSON、エラー出力先、終了コード、SDK経路を維持する。
helpには新オプションだけを追記。共通形式のparse/runtimeエラーはstdoutに完全なJSON1個、stderrは空。
parseエラーはcode2、実行/変換失敗はcode1。not_foundとunverifiedはcode0。
--jsonの重複は接続前にblocked。Monitorの不正引数もmainの境界で共通エラーになる。
status timeoutは既存Box<dyn Error> APIと「status RPC timeout」の表示文を維持しつつ、
型で識別できるようにした。provider文字列からtimeoutを推測しない。

## 出力量と秘密情報

共通形式のシリアライズ出力は最大1MiB。制限付きwriterで書き、超過時の部分JSONは捨て、
短い固定blockedレポートだけを出す。元のSDK HTTP受信サイズやプロセスRSSの上限ではない。
一覧は最大256件。total_entries/truncatedを明示し、切詰め履歴のnext_beforeはnullにする。
省略されたデータを飛ばす再開カーソルとして誤用させない。追加取得はしない。

公開address/signatureはSDK型で構文検証し、許可した数値・bool・固定enumだけを要約へ投影する。
生Account Data、Tx message/log、provider error message/body、任意context文字列を流さない。
Txエラーはerror_presentと既存分類で表し、任意のエラー本文を出さない。
これは今回の共通出力経路の制約であり、変更していないlegacy診断exampleを再監査したとの主張ではない。

## 変更ファイル

- src/readonly_report.rs：共通型・安全なadapter・UTC・制限付きrender。
- src/readonly_report/tests.rs：共通仕様と既存データ/Mockによる検証。
- tests/readonly_report_cli.rs：実CLIの既定互換/明示JSON/不正引数。ネットワーク実行はしない。
- src/bin/devnet_account.rs、src/bin/devnet_status.rs：--jsonの薄い出力境界。
- src/monitor/cli.rs、src/main.rs：既存Monitorの明示JSONとparseエラー境界。
- src/status_readonly.rs：表示文を保った型付きtimeout。
- src/lib.rs：readonly_report公開1行。
- 本書と結果JSON。Cargo.toml/Cargo.lock、独立Transport、account query、SDK、AGPは変更しない。

## 検証

| 対象 | 一意成功 | 失敗 |
|---|---:|---:|
| 新規共通仕様unit | 18 | 0 |
| 新規実CLI | 5 | 0 |
| 既存Monitor CLI | 12 | 0 |
| 既存account要求unit | 1 | 0 |
| 既存account回帰 | 12 | 0 |
| 既存status回帰 | 9 | 0 |
| 既存Phase 7Z結合 | 11 | 0 |
| 合計 | 68 | 0 |

レビューで提供済みデータのquery_attempted表示とMonitor変換エラーの分類を補強し、
影響する新規4件とstatus timeout回帰1件だけを再確認した。延べ73実行・73成功、重複を除き68件。
UTC/境界/null/全status、RPC code、秘密文字列除外、個別slot、未知Tx、切詰め、出力上限、
not_found+HTTP200、既定CLIと新JSONの分離、Mockで同じ4照会のままであることを検証。
全ビルド/テストは既存Linux seccompを継承してsocket生成を拒否。専用HOME/TMPDIR、
cargo --offline --locked、並列2。既存署名系、live RPC、HTTP模擬server試験の反復はない。
対象rustfmt --check、git diff --check、最終cargo check --offline --lockedも成功した。
個別のテスト名・実行数・log/source hashは[結果JSON](cdk-readonly-json-results-2026-11-01.json)に保存した。

## 未検証・拡張

新形式のDevNet/Testnetオンライン結果は未検証。過去の独立Transport online PASSを新形式のPASSへ転用しない。
既存SDKの非制限HTTP経路、JSON解析メモリ/RSS/CPU、Dataの実在正常系の未検証事項はそのまま。
独立Transportの上限・allowlist・redirect/retry制限はbyte単位のソース一致で保全する。
AGP/Telegram・SDKソース、既存未コミット成果物も開始時hashで照合する。
GitHub/Notionへの記録は今回行わない。

今後の拡張は新しいRPCではなく、既存の型に対応する安全な投影adapterを追加する形にする。
自由なValue/error文字列をそのまま受け取る汎用wrapperは公開しない。
今回の対象外だった診断exampleや人間向けモードへの展開は、利用要件がある場合に別途判断する。
