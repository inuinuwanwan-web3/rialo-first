# Phase 7V — Signing Compatibility & Fee Estimation Investigation

## 判定・調査範囲（2026-10-18 JST）

調査・必要最小限のオフライン検証は完了。CLI/CDK一致とTestnet受理互換性、
手数料0のサーバー側原因は未検証として残す。実送金の準備完了ではない。
AGENTS.mdは祖先・リポジトリ内に見つからなかった。main/HEAD ec38892。
Phase 7P〜7U、7O結果、pre_sign/PreparedTransfer、unsigned_tests、Cargo.toml/lock、
ローカルCDKを確認。CLIは `rialo --version` で0.18.1-2e1fdefe31cc。
Cargo.toml:7、Cargo.lock:3204はCDK0.18.1。既存未コミット変更は保持した。
7U開始時のハッシュ記録と比較し、今回の変更前のsrc/Cargoファイルに差分0を確認。

## 署名互換性の切り分け

依存ソース基点：/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/

| 項目 | 根拠 |
| --- | --- |
| 構築 | src/lib.rs:28/29/83、正式TransactionBuilderとadd_transfer_instruction、build_transaction |
| 未署名Transaction | rialo-cdk-0.18.1/src/transaction/builder.rs:529、必要署名数のゼロ枠を生成 |
| message_data | rialo-s-transaction-0.18.1/src/lib.rs:666は別SDK型のmessage().serialize() |
| 署名対象 | src/lib.rs:107、CDK builder.rs:107/488。buildした正確なmessage bytes |
| シリアライズ | CDK builder.rs:645/705、messageとcompact署名配列＋messageのwireは別 |
| アルゴリズム | CDK keyring/traits.rs:24/110はEd25519。今回は実Keyringを使わない |
| 検証 | src/lib.rs:113/126、公開鍵照合とverify_strict後に署名を組み込む |
| TX ID | CDK rpc/client.rs:524/545はRPC返却signature。SDK sanitized.rs:162は先頭署名アクセサ。CLI/現ノードの算出規約は未証明 |

A：CDK内部の構築・合成鍵署名・検証・wire往復は既存テストで確認済み。
src/lib.rs:150以降、src/unsigned_tests.rs:18/48/73を再利用し、重複追加しない。
B：CLI/CDK byte一致は未検証。Phase 7Tのhelp調査で安全な未署名exportや
valid_from/config固定手段を確認できず、Phase 7R/Sでも正式fixture/ソースが不足。
C：現在のTestnetが受理する署名形式との一致は未検証。過去CLIの送金実績や
MockテストだけではCDK実取引の受理を証明しない。

今回CLIから新しい取引データを取得していない。同じ探索・Web検索は繰り返さず、
固定入力の比較材料が不足しているという結論を維持する。
署名対象SHA-256やゼロ署名枠をTX IDとは扱わない。

## 手数料データ経路とエラー処理

1. src/pre_sign.rs:274は正確なsigning_messageをbase64にしてReadRpc::feeへ渡す。
2. :222はCDK get_fee_for_messageのResultを `?` で伝播して.valueを返す。
3. CDK rpc/client.rs:766はversion=0のgetFeeForMessageリクエストを作り、
   応答valueを変換せずFeeResponseへ移す。context/versionはこの簡易型に保持されない。
4. rialo-api-types-0.18.1/src/messages/get_fee_for_message.rs:16はbase64 bincode
   VersionedMessage、:29はOption<u64>。Some(0)、None、Errは別状態。
5. CDK rpc/client.rs:409以降はHTTP/JSON/RPCエラーをErrとする。エラーコード取得の
   unwrap_or(0)はエラー番号の補完であり、手数料0への変換ではない。
6. preparation.rs:101/107は成功したfeeだけを保存。:124の初期値はNone。
   エラーは:106で伝播し、:167のReportにはNoneが残る。
7. pre_sign.rs:277はNoneを未取得エラーとし、Some(0)は数値見積もりとして扱う。
   preparation.rs:71/79はNoneをUNKNOWN、Some(0)を0として表示する。

現在コードのこの経路に、RPCエラーや欠損を手数料0へ変換する処理はない。
新規テストでもその区別をReportの表示まで確認した。

## 0 kelvinについて言えること・言えないこと

| 仮説/観測 | 判定 |
| --- | --- |
| アプリがRPCエラーを0へ変換 | 現在の経路では否定。コード確認とMock試験でErr/NoneはBLOCKED・UNKNOWN |
| RPCが0を返した | 7O文書:34/47には見積もり0との記録がある。この経路ならSome(0)を受けた表示と整合するが、当時のraw JSONを再確認したものではない |
| 入力message不完全 | 現在の構築はcanonical VersionedMessage・required_signatures=1を確認。署名配列がないこと自体はfee APIの仕様に合う。ただし当時のraw message/ノード受理条件は未検証 |

未署名messageと完成TXの違いは外側の署名配列。必要署名数はheaderにあり、
未署名だから0になるわけではない（unsigned_tests.rs:73）。
署名後もmessageが不変ならfee照会payloadは同じだが、時刻やノード状態を跨ぐ
応答の同一性・実際の請求額の同一性は保証できない。

config prefixとvalid_fromはmessageに含まれる。アプリのTTL・prefix再照合は
安全チェックであってサーバーの料金式ではない。最新ブロック高はこのmessageや
feeリクエストに別フィールドとして送っていない。ノード側のconfig/高さ/受理期間が
料金に与える影響、SDK/サーバー版差の影響は仕様不足で未検証。

rialo-s-fee-structure-0.18.1/src/lib.rs:147/151/160にはdeprecated関数と
kelvins_per_signature=0ならゼロ料金を返すコードがある。しかしTestnet稼働実装との
対応は不明なので根因に採用しない。過去CLIの5000との差も未解決。
結論：現在のクライアントの0/エラー混同は除外できるが、7Oの0発生原因は未特定。
手数料上限は見積もり/残高検査値で、実際の徴収上限や無料送金保証ではない。

## 新規テスト・安全監査・結果

変更：src/pre_sign/preparation.rs:330のテスト
zero_missing_and_rpc_error_remain_distinct_in_reportを1件追加。本番処理変更なし。
実API応答型に合成JSONの0/5000/nullを復号し、合成Errと合わせて既存準備経路を
通す。数値・欠損・エラーのReportと表示の違いを検査する。
期待値は現在の型/制御フローに基づき、ノードの料金式を推測したものではない。
HTTP transport自体を実行した試験ではない。

実行前監査：新規テストは固定公開アドレスとMock時刻/prefix/balanceだけ。
秘密鍵生成・読取、署名、ファイル、環境変数、RPC/送信クライアントを使わない。
7Uの既存134件は変更なし、新規1件をA分類とした（A52、条件確認済みB83、C0）。
専用mkdtemp/TMPDIRとPATH・TMPDIR・CARGO_NET_OFFLINE・LANGのみの環境を使用。
既存テストのメモリ内合成署名だけを今回の許可に従って実行した。

- cargo fmt --check：最初は新規format!の改行で失敗。追加箇所のみ整形し再実行成功。
- cargo check --offline --locked：成功。
- cargo test --offline --locked：既存134＋新規1＝135成功、0失敗、未実行0。
  library125、binary10、doctest0。ループ4ケースを4テストとして重複計上しない。
- git diff --check：成功。並列ロック競合再発なし。
- proc-macro-error2 v2.0.1の既存将来互換性警告は継続。

## 不足資料・実送金までの優先順位

1. 対象CLI版の公式未署名fixtureまたは安全なexport手順を配布元に照会する。
   from/to/payer、amount kelvin、valid_from、config prefix、OCC、命令/account順序、
   message/wire期待値とビルド版が必要。今回照会は送信していない。
2. TestnetのgetFeeForMessageサーバー仕様・版・0条件を確認する。
   将来、別途承認された読み取り検証で正確なbase64 message、decoded header、
   request version、raw response/context、取得時刻・configを対応付けて保全する。
   今回は禁止のため追加RPCは実行していない。
3. 正式な署名者接続・公開鍵照合、ネットワーク受理期間、最新残高と見積もり、
   一度だけの送信と結果不明時の自動再送禁止、履歴の巻き戻しリスクを確認する。
   実署名・送信には別の明示承認が必要。

次の最小作業は1の必要資料を配布元へ照会すること。コードを増やして証拠不足を
埋めない。今回は実ウォレット・実秘密鍵・外部通信・実署名・TX送信・Faucet、
commit/push、Notion記録を実施せず、Phase 7Vで停止する。
