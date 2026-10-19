# Phase 7S — Official CLI Source Verification & Compatibility Evidence

## 判定・調査日時

2026-10-17（JST）。公開Webページの閲覧・検索とローカル読取のみ。
調査は完了、署名互換性は未検証。分類C：対象版の正式CLIソースを
今回の公開閲覧では取得できなかった。不在・非公開の断定ではない。
コード変更なし。今回の新規ファイルは本文書のみ。

## ローカル確認

対象・祖先およびリポジトリ内にAGENTS.mdは見つからなかった。
git status/diffでsrc/lib.rs末尾の3行追加と未追跡のunsigned_tests、
Phase 7P/7Q/7R文書、phase-7-signing-investigation.mdを確認し、保持した。
Phase 7P:14以降、7Q:34以降、7R:11以降の根拠を再確認した。

- `rialo --version` → `rialo 0.18.1-2e1fdefe31cc`
- Cargo.toml:7、Cargo.lock:3204：CDK 0.18.1。
- `/home/pc/.local/share/rialo/releases/stable/0.18.1/manifest.json:5`：
  CLI commit `2e1fdefe31cc6569006a4d5a96a2316d3ecc6c93`。
- registry基点は `/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`。
  rialo-cdk-0.18.1/.cargo_vcs_info.json:3にも同じSHA、:4はdirty=true、
  :6はdeveloper-frameworks/cdk/rialo-rs-cdk。
- 同Cargo.toml:26はrepositoryとしてhttps://github.com/SubzeroLabs/rialoを記載。
- src/lib.rs:83はbuild_transaction、:85はVersionedMessage復号、:107は
  正確なtransaction.message()を返す。今回呼び出し・署名はしていない。

## 公開元・確認URLと結果

| URL | 出所・閲覧結果・限界 |
| --- | --- |
| https://www.rialo.io/ | Rialo公式サイト。Dev Portalへのリンクを辿った |
| https://www.rialo.io/for-devs | 公式Dev Portal。「Read the Docs」がdocs.rialo.ioを参照 |
| https://docs.rialo.io/ | Cloudflare Accessの認証ページへ転送され本文取得不能。認証や回避は実施せず、転送先のクエリ値は記録しない |
| https://github.com/SubzeroLabs | 公開組織ページは閲覧可能。表示された公開一覧に対象rialo本体は確認できず。動的読込エラーもあり完全一覧の保証なし |
| https://github.com/SubzeroLabs/rialo | ローカルパッケージが示す候補。閲覧ツールは404を返した。不存在・非公開・移転の区別はできない |
| https://github.com/SubzeroLabs/rialo/commit/2e1fdefe31cc6569006a4d5a96a2316d3ecc6c93 | 対象SHAを確認するため閲覧、取得エラー。コミット実在を公開資料で検証できず |
| https://github.com/SubzeroLabs/rialo/tree/v0.18.1 | タグ名の候補を検査しただけ。取得エラーでタグの存在・正式名称は未確認 |
| https://docs.rs/rialo-cdk/0.18.1/rialo_cdk/ | 対象版の公開API候補。閲覧ツールのCache miss、内容未確認 |
| https://docs.rs/crate/rialo-cdk/0.18.1/source/src/transaction/builder.rs | 対象版ソース表示候補。ツールで取得不能。ページ不存在とは断定しない |
| https://doc.rust-lang.org/cargo/commands/cargo-package.html | Rust/Cargo公式仕様。.cargo_vcs_info.jsonとdirtyの説明を確認 |

補助検索で https://www.npmjs.com/package/%40rialo/ts-cdk と
https://www.npmjs.com/~subzerolabs が得られ、getFeeForMessageの記述と
0.18.1の表示があった。ただし今回の公式サイトからの参照関係を確立できず、
Rust CLIの公式仕様・対象版の呼び出し経路の証拠として採用していない。
検索に混入した別製品Railo/RiloやSolanaの仕様もRialo仕様として扱わない。

検索語には対象完全SHAの短縮形、Rialo CLI GitHub、
site:rialo.io getFeeForMessage、site:github.com/SubzeroLabs transfer_command、
Rialo 0.18.1 test vectorを使用。正式な対象版fixture、期待バイト列、
署名済み公開サンプルを比較根拠として確認できなかった。
検索漏れや閲覧制限があり、公開資料が存在しないという証明ではない。

## コミット・dirtyの意味

検証済みなのはローカルメタデータの版・SHA一致まで。
コミットが示すリポジトリはCDK Cargo.toml上の候補であり、今回公開ソースと
対応付けできなかった。CLIのビルド対象コード、依存lockfile、ビルド時差分、
版間の仕様変更は未検証。

Cargo公式の「.cargo_vcs_info.json format」は、dirtyがパッケージ作成時の
Git worktreeのdirty状態を示すと説明する。またVCS情報はbest-effortで、
パッケージの出自やソースとVCS情報の一致を保証しないと明記している。
https://doc.rust-lang.org/cargo/commands/cargo-package.html#cargo_vcs_infojson-format

したがってdirty=trueは現在の利用者リポジトリの汚れを意味しない。
未コミット変更のファイル・内容・署名処理への影響はこのJSONから判別できない。
CLIもdirtyでビルドされたとは推定せず、同じSHAだけでソース/バイナリ同一性や
署名互換性を断定しない。

## 署名互換性

CDKのローカル実装についてはPhase 7Qの確認結果を維持する：
rialo-cdk-0.18.1/src/transaction/builder.rs:529のbuild_transaction、
:107のmessage、:488の署名対象、:66の署名配置、:645/:705のmessage/wire形式。
rialo-s-transaction-0.18.1/src/lib.rs:666のmessage_dataは別型のAPI。
CDK keyring/traits.rs:24/110はEd25519。

CLIの正式な呼び出し経路と入力条件が得られていないため、
build_transaction/message/message_data、Ed25519使用方法、署名付与、
シリアライズ、Chain識別、TX IDの相互一致はすべて未検証。
同じ関数名・シンボル・SHAをバイト列一致の代用にしない。
新規比較テストや署名/送信機能は追加していない。

## 手数料0 kelvin

ローカルrialo-api-types-0.18.1/src/messages/get_fee_for_message.rs:16は
base64化したbincode VersionedMessageを入力とし、:29はOption<u64>の応答。
署名配列を持つTX全体を入力とする仕様ではない。
同じmessageに外部署名を付けてもmessage自体を変更しなければ照会payloadは
同じになる、というのはこの入力構造からの判断。異なる時点のサーバー応答や
実請求額が一致する保証ではない。

rialo-s-fee-structure-0.18.1/src/lib.rs:147/151/160にはdeprecated関数と
kelvins_per_signature=0時のゼロ料金経路があるが、現Testnetで利用される証拠はない。
公式Dev Portalの低額/ゼロ料金という一般的説明も、getFeeForMessageの
0条件、0.18.1のTestnet設定、CLI料金計算式を示していない。
https://www.rialo.io/for-devs

正式な現行RPCサーバー計算式・CLI手数料経路・当時のノード設定が取得できず、
Phase 7Oの0の原因は不明のまま。無料送金や実手数料上限保証とは扱わない。

## 検証・次の最小作業

文書のみ追加。テスト追加0、実行0（134件の今回の成功を主張しない）。
git status --shortとgit diff --checkを確認。既存ファイルは変更していない。
公開ページ閲覧・検索以外の外部操作、clone/pull、ファイルダウンロード、
インストール、認証API、実ウォレット操作、署名、TX送信、Faucet、
commit/push、Notionへの記録は行っていない。

次の最小作業は、配布元への照会内容を以下に絞ること（今回は送信しない）：

1. CLI 0.18.1、SHA 2e1fdefe31cc6569006a4d5a96a2316d3ecc6c93の
   公開transfer/message生成ソース、依存lockfileとビルド差分。
2. 公開困難なら同版の公式未署名fixture。from/to/payer、kelvin金額、
   valid_from、config prefix、OCC、命令/account順序と期待message/wireを含むもの。
3. CDK dirty差分と、Testnet getFeeForMessageが0を返す条件・サーバー版。

代替検証は公式fixtureを得た後のオフラインbyte比較とする。
対象資料が得られるまで同じ探索を繰り返さず、Phase 7Sで停止する。
