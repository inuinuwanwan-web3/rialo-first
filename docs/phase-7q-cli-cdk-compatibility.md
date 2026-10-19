# Phase 7Q — CLI / CDK Signing Payload Compatibility

## 結論・調査範囲

2026-10-17：ローカル調査と未署名テストまで実施。CLIの正式な比較資料が
不足しているため、CLI/CDK互換性の確認は未完了として停止する。
外部通信、実ウォレットアクセス、署名、署名済みデータ生成、TX送信、
Faucet、署名状態変更、commit/push、Notion記録は実施していない。

対象・祖先のAGENTS.mdは見つからなかった。開始時の変更はsrc/lib.rs、
未追跡src/unsigned_tests.rs、docs/phase-7p-transaction-construction.md、
docs/phase-7-signing-investigation.md。これらは変更・削除していない。
今回追加したファイルは本文書だけ。新規テストは0件。

## CLIソースとバージョン

ローカルの `rialo --version` は `rialo 0.18.1-2e1fdefe31cc`。
実行ファイルは `/home/pc/.local/share/rialo/bin/rialo`。
リポジトリ、CLIインストールディレクトリ、Cargo registryのソース探索では
CLI送金コマンド本体は見つからなかった。端末全体での不存在を証明したものではない。
Cargo.toml:7はrialo-cdkを0.18.1に固定しており、Cargo.lockも同版。

以下の依存ソースの基点は
`/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`。

- rialo-cli-representation-0.18.1/src/lib.rs:4：CLI表示用trait。
- rialo-cli-representable-0.18.1/src/lib.rs:4：表示用derive macro。

これらはtransfer構築・署名実装の証拠ではない。
docs/2026-10-11-testnet-transfer-e2e.md:72以降はCLI実送金の記録だが、
同一条件で比較できる未署名message/wire、構築時刻、完全な入力fixtureがない。
CLIの利用SDK、鍵指定の内部処理、署名payload、TX ID規約は未検証。

## CDKの正式な署名対象

以下のbuilderはrialo-cdk-0.18.1/src/transaction/builder.rsを指す。

| 確認項目 | ローカルソースの根拠と結果 |
| --- | --- |
| build_transactionの戻り値 | builder:529、Result<Transaction>。必要署名数だけゼロ枠を配置 |
| Transaction構造 | builder:20、signatures: Vec<Vec<u8>> と message: Vec<u8> |
| 正確な署名対象 | builder:488、build()のmessage_bytesをそのまま署名関数に渡す。今回は呼び出していない |
| 取得API | builder:107、message() -> &[u8]。アプリsrc/lib.rs:107 signing_messageが再利用 |
| message_dataとの区別 | rialo-s-transaction-0.18.1/src/lib.rs:666の別Transaction型に存在し、message().serialize()を返す。CDK Transactionのメソッドではない |
| アルゴリズム | rialo-cdk-0.18.1/src/keyring/traits.rs:24/110、Ed25519。コード読取のみ |
| 署名配置 | builder:66、署名者account_keysの位置に対応する枠へ配置。これだけでは暗号学的正当性を検証しない |
| message形式 | builder:645、header、compact account配列、valid_from LE i64、config prefix LE u64、OCC、compact命令配列 |
| wire形式 | builder:705、compact署名配列（各64バイト）＋message。正式API名はserialize_wired |
| Chain/Network | builder:663、config prefixは署名対象。RPC URLやTestnet文字列は含まれず、恒久的chain IDとしての保証はない |
| TX ID | rialo-cdk-0.18.1/src/rpc/client.rs:524はRPC返却signatureを扱う。rialo-s-transaction-0.18.1/src/sanitized.rs:162は先頭署名を返す。CLI/稼働ノードの同一規約は未証明 |

未署名messageのSHA-256をTX IDとは呼ばない。ゼロ署名枠も有効な署名ではない。
src/lib.rs:59のPreparedTransferは正式Builderを使い、VersionedMessageの
復号・canonical再シリアライズ一致も確認する。新しいTX形式は追加していない。

## CLIとの比較可能性

CDK内の構造・署名対象・シリアライズは確認済みだが、CLIの対応する実装・
fixtureがないため、6項目すべてのCLI/CDK一致は未検証。
同じバージョン番号や過去のCLI送金成功だけでは互換性の証拠としない。

将来の比較にはfrom/to、amount kelvin、valid_from、config prefix、OCC、
命令とアカウント順序、payer、必要署名数を固定した正式な未署名fixtureが必要。
手数料上限は現在のアプリの判定値で、message内の強制上限ではない。
アプリのチェック期限とネットワークのTX受理期間も別である。
Phase 7Oの見積もり0 kelvinの根因は今回も未解決で、無料保証とはしない。

## オフライン検証

今回実行して成功したもの：

- cargo fmt --check
- cargo check --offline --locked
- cargo test --offline --locked unsigned_tests：5成功、0失敗
- git diff --check

src/unsigned_tests.rs:18は構造・wire往復、:48は同一入力の再現性と
金額・時刻・prefix・宛先変更、:73は手数料payloadとwireの区別、
:89は不正wire、:99は無効な入力の拒否を検証する。
これらはCDK内部の未署名テストであり、CLI互換性テストではない。
新規テストは重複を避け0件。今回5件成功、残り129件は未実行。

既存構成はlibrary124件＋binary10件＝134件。
無指定のcargo test --offline --lockedには合成鍵で署名済みデータを作る
テストが含まれるため、今回の「署名済みデータの生成禁止」を優先して実行していない。
134件全成功はPhase 7Pの記録値であり、今回の全件成功とは報告しない。
ロック競合の修正・過去の並列反復結果は既存記録を確認したが、今回の5件は
永続化テストを含まないため、ロック競合の再検証には該当しない。
proc-macro-error2 2.0.1の将来互換性警告1種類は既存依存由来で、今回の変更とは無関係。

## 完了条件・次の最小作業

| 条件 | 判定 |
| --- | --- |
| A CLI構築方式 | 未検証：本体ソース不足 |
| B CDK署名対象 | 検証済み：ローカル実装と未署名テスト |
| C CLI/CDK署名対象比較 | 未達成：正式比較データ不足 |
| D CLI/CDKバイト列一致 | 未検証 |
| E オフラインテスト | 未署名5件は検証済み。全134件再実行は未達成 |
| F 実署名・送信なし | 検証済み：今回署名済みデータも生成していない |

次の最小作業は、CLI 0.18.1-2e1fdefe31ccに対応する公式transfer構築ソースと
依存バージョン、または入力条件付きの公式未署名fixtureをオフラインで入手すること。
代替検証はCLIを推測で再現せず、そのfixtureと既存CDK messageのbyte比較を行う。
実署名にはさらに正式な署名者接続仕様、公開鍵照合、ネットワーク有効期間と
最新の残高・手数料確認、明示的承認が必要。今回は資料不足で停止し、次Phaseへ進まない。
