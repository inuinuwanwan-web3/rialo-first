# Phase 7W — Official CLI / Fee Evidence

## 調査範囲と結論（2026-10-18 JST）

Phase 7Wのローカル証拠確認・記録は完了。CLI/CDK互換性は未検証、
Testnet手数料0の原因は未特定。実送金準備完了とは判定しない。
main、最新コミットec38892。既存のsrc/lib.rs、preparation.rsの差分、
未追跡のPhase 7P〜7V文書・unsigned_tests・署名調査文書を保持した。
祖先・対象・リポジトリ内のAGENTS.mdは見つからなかった。
Phase 7P〜7V、署名/構築/手数料処理、ローカル依存ソース・manifestを確認。
外部通信禁止のため、公式Webの新規閲覧や過去のアクセス可否の再確認はしていない。

## バージョンと証拠の出所

- 実行結果 `rialo --version`: `rialo 0.18.1-2e1fdefe31cc`。
- Cargo.toml:7 / Cargo.lock:3204：rialo-cdk 0.18.1。
- CLI実体はPhase 7R:18に記録された
  /home/pc/.local/share/rialo/releases/stable/0.18.1/bin/rialo。
- 同ディレクトリmanifest.json:5は
  `2e1fdefe31cc6569006a4d5a96a2316d3ecc6c93`。
- 以下の依存ソース基点は
  /home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/。
  rialo-cdk-0.18.1/.cargo_vcs_info.json:3/4は同じSHAとdirty=true。
  SHA一致は検証済みだが、ビルド時差分・CLI依存lockfileは未確認。

ローカルの配布パッケージ実装は現在利用しているコードの証拠であり、
現在のTestnetノード実装の証拠とは別。公式公開資料の過去の閲覧結果は
Phase 7S:28以降を参照し、今回の検証結果に置き換えない。
同版・同SHA・シンボル名だけで互換性を認定しない。

## 署名仕様の分類

| 分類 | 項目・根拠 |
| --- | --- |
| 検証済み | src/lib.rs:28/29/83の正式Builder経路、:107のmessage取得 |
| 検証済み | rialo-cdk-0.18.1/src/transaction/builder.rs:529はResult<Transaction>とゼロ署名枠を生成 |
| 検証済み | 同:107/488はbuildしたmessage bytesを取得/署名入力とする。コード読取のみで署名未実行 |
| 検証済み | rialo-s-transaction-0.18.1/src/lib.rs:666のmessage_dataは別SDK型のmessage().serialize() |
| 検証済み | CDK builder.rs:645はheader/account/valid_from/config prefix/OCC/命令、:705はcompact署名配列＋message |
| 検証済み | CDK keyring/traits.rs:24/110はEd25519、src/lib.rs:113/126は公開鍵照合とverify_strict |
| 検証済み | CDK rpc/client.rs:524はRPC返却Signatureを扱う。SDK sanitized.rs:162は先頭署名アクセサ |
| 未検証 | CLIの実際の構築・署名呼出経路、同一入力の署名対象長/バイト列/wire、TX ID算出規約 |
| 未検証 | 現TestnetでのCDK取引受理、config prefixの恒久的chain IDとしての保証 |
| 不一致 | CLI/CDKで証明できた不一致は0件。比較不能なので一致の証拠でもない |

CLI側の安全な取得不能の理由：Phase 7T:13のhelp調査では未署名export、
build-only、時刻/config固定オプションが確認できなかった。JSONは結果の出力形式で、
transferは送信操作と記載されている。秘密鍵やRPCなしにmessageを得られるとは
保証できない。未公開機能の不存在までは断定しない。
Phase 7R/Sの調査では対象版の公式fixture・本体ソースが不足し、新しい比較資料も
今回の作業対象に存在しない。同じ探索やtransferの試行は行わない。

## 手数料0の分類

| 分類 | 根拠・判断 |
| --- | --- |
| 検証済み | src/pre_sign.rs:222/274はsigning_messageのbase64をgetFeeForMessageへ渡す |
| 検証済み | rialo-api-types-0.18.1/src/messages/get_fee_for_message.rs:16はbincode VersionedMessage、:29はOption<u64> |
| 検証済み | CDK rpc/client.rs:766はversion=0で照会、valueをそのまま返す。context/versionは簡易FeeResponseでは保持しない |
| 検証済み | 同:409以降はHTTP/JSON/RPCエラーをErrとする。エラー番号のunwrap_or(0)は手数料0変換ではない |
| 検証済み | preparation.rs:101/124/167はNone初期化・成功時だけfeeを保存。:71でNoneはUNKNOWN、Some(0)は0表示 |
| 検証済み | pre_sign.rs:265/268はcanonical形式/対応Legacy/prefixを検査。:277はfee欠損を停止。Some(0)は数値として扱う |
| 未検証 | 未対応messageを現在のノードがErr/None/その他のどれで返すか。ローカル型の表現能力からノード仕様は決めない |
| 未検証 | 7O当時のraw message、raw JSON応答、サーバー版・料金設定、有効期間/config/高さの影響 |
| 未検証 | Testnetで正常な見積もり0を返す正式条件、実徴収額、SDK版差の影響 |

未署名messageと完成TXは署名配列の有無で異なる。fee APIが要求するのはmessage。
required_signatures=1をunsigned_tests.rs:73で確認しており、未署名を理由に
必要署名数0と説明することはできない。
クライアントがエラーを0に変換する仮説は現在の経路では否定できる。
ただし7O文書:34/47の「0」は過去の結果記録であり、生応答の独立再検証ではない。

ローカルrialo-s-fee-structure-0.18.1/src/lib.rs:147/151/160のdeprecated関数には
kelvins_per_signature=0の分岐があるが、現Testnetの利用証拠がないため原因としない。
正しい照会形式を満たすこととノード料金式の確認は別。0を無料送金保証にしない。
手数料上限はアプリの見積もり/残高判定用で、実徴収の強制上限ではない。

## オフライン確認

新規テスト0、コード変更0。既存テストの安全な部分だけ実行した。
- cargo test --offline --locked unsigned_tests：5成功。
- cargo test --offline --locked pre_sign::preparation::tests::zero_missing_and_rpc_error_remain_distinct_in_report -- --exact：1成功。

合計6成功・0失敗。既存構成はlibrary125＋binary10＝135件。
残り129件は今回未実行。署名実行禁止のため全件の合成署名テストは起動しない。
7Vの135成功は過去の結果として区別する。
上記6件は公開入力/Mockのみで、署名・鍵生成・RPC・永続化操作を呼ばない。
新規専用TMPDIR、許可リスト環境（PATH/TMPDIR/CARGO_NET_OFFLINE/LANG）で実行。
proc-macro-error2 v2.0.1の既存将来互換性警告あり。git diff --checkは成功。
今回の変更ファイルは本文書のみ。既存変更は保持。

## 実送金までの残作業と次の最小作業

1. 対象CLI版の公式未署名fixtureまたは鍵/RPC不要のexport手順を入手する。
   from/to/payer、整数kelvin金額、valid_from、config prefix、OCC、命令/account順序、
   期待message/wire、正確なビルド版を揃え、オフラインで比較する。
2. getFeeForMessageのノード実装版・0条件・受理期間を確認する。
   必要なら別途承認された読取でrequest/response/contextを対応付けて保全する。
3. 正式な署名者接続、公開鍵照合、送信/結果確認、結果不明時の自動再送禁止、
   保存履歴の巻き戻し対策の限界、最新残高と見積もりを確認する。
   実署名・送金には別の明示的承認が必要。

次の最小作業は配布元への資料照会。理由は、ローカルの追加テストではCLI入力や
ノード料金設定という不足証拠を作れないため。照会にはCLI0.18.1、完全SHA、
CDK dirty差分の質問と上記fixture要件・fee0条件をまとめる。
今回は送信せず、外部通信・署名・TX・Faucet・commit/push・Notion記録なしで停止する。
