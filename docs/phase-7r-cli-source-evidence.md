# Phase 7R — CLI Source Discovery & Signing Compatibility Evidence

## 1. 判定と保全（2026-10-17）

ローカル資料の調査は完了。正式CLI構築ソース・未署名fixtureが不足し、
署名互換性の検証は未完了として停止する。コード・テストは変更していない。
今回の追加は本文書のみ。既存のsrc/lib.rsの3行追加、src/unsigned_tests.rs、
Phase 7P/7Q文書、docs/phase-7-signing-investigation.mdは保持した。
対象・祖先およびリポジトリ内のAGENTS.mdは見つからなかった。

## 2. CLIの実体・ビルド証拠

実行コマンドと出力：

```text
command -v rialo
/home/pc/.local/share/rialo/bin/rialo
readlink -f /home/pc/.local/share/rialo/bin/rialo
/home/pc/.local/share/rialo/releases/stable/0.18.1/bin/rialo
rialo --version
rialo 0.18.1-2e1fdefe31cc
```

`file` と `readelf -n` を実体ファイルに実行：Linux x86-64 ELF、not stripped、
GNU Build ID `b41bd2ccf1f4938bb3e350727baad35448918891`。
これはローカルバイナリの識別情報で、配布元署名の検証ではない。

`/home/pc/.local/share/rialo/releases/stable/0.18.1/manifest.json`：

- :3 stable、:4 version 0.18.1
- :5 commit `2e1fdefe31cc6569006a4d5a96a2316d3ecc6c93`
- :6 built_at `2026-08-18T22:42:16Z`
- :9 archive `binaries-stable-latest-linux-amd64.tar.gz`
- :15/:28 同梱バイナリ `rialo` / `bin/rialo`

manifestは配布物を識別できるが、CLIのCargoパッケージmanifestやlockfileではない。
Cargoパッケージ名・完全な依存解決結果は未確認。

## 3. ローカル探索と新しい手掛かり

確認した範囲：リポジトリ、CLIリリース配下、Cargo registry src/cache、
CDK Cargo.toml/README/.cargo_vcs_info.json、アプリCargo.toml/lock、
Phase 7P/7Q、過去CLI送金記録。標準Cargo Gitキャッシュ
`/home/pc/.cargo/git` は存在しない（test -dの終了コード1）。
既知のこれらの範囲にCLI本体ソースや公式のCLI未署名fixtureは見つからなかった。
端末の全ディレクトリで不存在を保証したものではない。
秘密鍵・ウォレット設定・認証情報のファイルは開いていない。

依存ソースの基点は
`/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`。

- rialo-cdk-0.18.1/Cargo.toml:14/15：パッケージ名と0.18.1。
- 同:26：repository値 `https://github.com/SubzeroLabs/rialo`。
  ローカルメタデータに記載された取得先候補である。現在の公開可否、
  アクセス権、CLI本体の収録状況は外部確認していない。
- rialo-cdk-0.18.1/.cargo_vcs_info.json:3：CLI manifestと同じ完全SHA。
  :4は `dirty: true`、:6のpath_in_vcsは
  `developer-frameworks/cdk/rialo-rs-cdk`。
  SHA一致は新しい手掛かりだが、ソース同一性や依存関係の証明にはならない。
- rialo-cli-representation-0.18.1/src/lib.rs:4、
  rialo-cli-representable-0.18.1/src/lib.rs:4は表示trait/macroで、transfer本体ではない。

バイナリに `nm -C` を実行し、以下のシンボルを確認した（関数は実行していない）：

```text
rialo::commands::client_subcommand::transfer_command::transfer<...>
<rialo_s_transaction::Transaction>::message_data
```

transferの型情報には `rialo_s_keypair::Keypair` も現れる。
これらは実装探索の手掛かりであり、transferが当該message_dataを呼ぶこと、
その入力、SDKの厳密な版、CDKへの直接依存、署名方式の完全一致は証明しない。
シンボルから独自のCLI実装を再現していない。

## 4. 比較結果と不足資料

正式CLIソースが見つからなかったため、CLI/CDK比較テストは実施していない。
以下8項目はいずれもCLIとの一致が未検証：TX構築API、message生成、署名対象、
Ed25519使用方法、Chain/Network識別、シリアライズ、署名配置、TX ID算出。

CDK側だけの確認済み根拠はPhase 7Q文書:34以降に記載済み。
rialo-cdk-0.18.1/src/transaction/builder.rs:529はbuild_transaction、
:107はmessage、:488はbuild結果を署名に渡す処理、:645/:705はmessage/wire形式。
SDK側rialo-s-transaction-0.18.1/src/lib.rs:666はmessage_data。
これらの存在や共通SHAだけではCLIの実際の呼び出し条件を証明できない。

docs/2026-10-11-testnet-transfer-e2e.md:72以降にはCLI送金成功が記録されているが、
正式な同一入力の未署名fixtureではない。過去成功とCDK互換性を混同しない。

不足資料は、上記完全SHAに対応するCLI transfer/message生成コード、
CLI Cargo.tomlとCargo.lock、ビルド時差分、または公式の未署名fixture。
fixtureにはfrom/to/payer、amount kelvin、valid_from、config prefix、OCC、
命令・account順序、必要署名数、message/wireの期待バイト列が必要。
秘密鍵や署名済みTXを求めるものではない。

## 5. テストと安全条件

追加テスト0件。今回のテスト実行0件（成功0・失敗0、全134件は未実行）。
コード変更がなく正式比較資料もないため、STEP 5/6の条件に従い再実行しない。
Phase 7Pの134件成功とPhase 7Qの未署名5件成功は過去の記録値であり、
今回の成功件数には計上しない。git diff --checkは成功。
既存未コミット差分を破棄せず、本番機能・安全機構・テストを変更していない。
実署名、TX送信、Faucet、秘密情報取得、外部通信、署名状態変更、
Git commit/push、Notion記録は実施していない。

## 6. 次の最小作業・取得方法（未実行）

外部取得の承認後、まずメタデータ記載のrepositoryのアクセス可否と対象SHAの
存在を確認し、その版のソースアーカイブを取得する。latestや同名版だけで代用しない。
CLI transferモジュール、Cargo manifest/lock、関連message/署名実装を確認する。
リポジトリが非公開または配布ソースが不足する場合は、Rialo配布元へ上記CLI版・
完全SHA・Build IDを提示し、該当ソースとビルド差分、または上記項目を持つ
公式未署名fixtureの提供を依頼する。CDKパッケージのdirty=trueについても照会する。

取得後に入力を固定してバイト列比較を行う。資料取得まで同じ探索やCDK内部テストを
繰り返さず停止する。ネットワーク有効期間、手数料0の原因、実署名者接続も
引き続き未解決で、今回実署名の準備完了とは判定しない。
