# Phase 7L — Rialo Testnet Read-Only Connection Integration

## 環境・調査

2026-10-16 UTCに実行。結果整理時刻11:25:38 UTC。
Git差分・未追跡資料を保持。適用対象と祖先にAGENTS.mdは見つからなかった。
Cargo.toml/lockはrialo-cdk 0.18.1固定。今回依存追加・更新なし。
CLIは/home/pc/.local/share/rialo/bin/rialo、--version結果は
rialo 0.18.1-2e1fdefe31cc。CLIのウォレット設定や鍵ファイルは開いていない。
公開アドレスとTestnet endpointは既存docs/2026-10-11-testnet-transfer-e2e.mdを利用。
既存mainの準備照会はDevNet。今回は別の明示モードを追加し、既存動作を保持した。

## 接続方式とローカルAPI根拠

以下はインストール済み
/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rialo-cdk-0.18.1/
を参照した。

- src/rpc/client.rs:272 HttpRpcClient::new(url)。
  非localhostではTLS証明書検証が有効。設定引数はURLで、鍵ロード不要。
- 同:289 new_with_configはTLS設定。今回は危険な証明書許可を使わない。
- 同:409 callはJSON-RPC 2.0のid/method/paramsをHTTP POSTし、型付き応答へ変換。
  今回アプリから認証情報を与えずに成功。永続的な匿名アクセス保証ではない。
- 同:1005 get_config_hash_prefix → getRecentValidatorConfigHash。
- 同:574 get_block_height → getBlockHeight。
- 同:518 get_balance → getBalance。返り値は整数kelvin。
- 同:766 get_fee_for_message、:619 get_signature_statuses、
  :586 get_transactionは次段階で使用候補。今回呼んでいない。
- src/constants.rsにはmainnet/testnet/devnet/localnetの識別文字列がある。
  これは各ネットワークの現在の稼働を保証しない。今回はTestnetだけ接続。

CLIは独立した実行ファイルであり、今回の通信はCLI経由ではない。
新モードはアプリ内の実HttpRpcClientを直接利用する。
既存Mock統合テストは変更せず維持。Mock注入用コードを本番化していない。

## 実行と送信内容

事前にendpointとメソッド・公開アドレスを表示してから実行した。
実行コマンド：
./target/debug/rialo-first --testnet-readonly --address 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa

固定接続先：https://testnet.rialo.io/
送信内容：getRecentValidatorConfigHash、getBlockHeight、getBalanceの
JSON-RPC読み取り要求。getBalanceには上記公開アドレスを含める。
秘密鍵・seed・認証トークン・取引・署名は送っていない。
接続の成立は最初のconfig照会成功で確認する。

初回のサンドボックス内実行はconfig取得失敗、exit 1で停止。
制限外での読み取り接続の承認を求めて実行し、exit 0で完了した。
初回失敗の低レベル原因は、エラーを固定文言にしたため未確定。
自動リトライではなく、制限外での明示的な再実行。

## 実RPC観測結果

| 項目 | CDKの出力 |
| --- | --- |
| config hash prefix | 3882139547858051996 |
| block height | 677572 |
| 公開アドレス | 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa |
| 残高 | 0 kelvin = 0 RLO |

CDKの型付きJSON-RPC応答処理が上記値を返したことを確認。
生JSON応答全体やHTTPヘッダーは保存していない。
これらは別々の読み取りで、同一ブロックの原子的スナップショットではない。
config prefixはvalidator config由来であり、独立したgenesis/chain ID証明ではない。

2026-10-11の記録はprefix 11774740490753875893、height 31319418付近、
送金後残高9,995,000 kelvin。今回はいずれも異なる。
Testnetリセット・設定変更・資金移動等の原因は未調査。断定しない。
過去のCLI E2E成功記録を今回の残高観測で削除・修正しない。
現在の観測残高では0.001 RLO＋手数料の送金はできない。

## CLIとCDKの区別

A CLI接続：今回は未実施。--versionのみ。過去のCLI成功は既存文書の履歴。
B CDK接続：実Testnet endpointで成功。
C RPC読み取り：config prefix、block height成功。
D 公開アドレス残高：成功、0 kelvin。
E 実署名・TX送信：未実施。

## 実装・検証

変更：
- src/main.rs:18/28 明示的--testnet-readonly [--address PUBLIC_KEY]モード。
- src/main.rs:79 読み取り専用CDK実行、固定Testnet URL、段階別の固定エラー。
- src/main.rs:107 既存DevNet経路へ入る前に分岐。
- src/main.rs:286/303 オフライン引数テスト2件。
- README.mdに使用方法を追記。
- 本文書を追加。

不正アドレス、送信フラグ、endpoint上書きを通信前に拒否。
既存30秒タイムアウトを再利用。保存ストアを開かず、取引も構築しない。
新規ライブ接続を通常cargo testから呼び出すコードはない。

cargo fmt --check / cargo check --offline --locked /
cargo test --offline --locked / git diff --check：すべて成功。
既存118件＋新規2件＝120件成功、失敗0（library111、binary9）。
Phase 7Kの一過性LockUnavailableは今回の実行では再現しなかった。
根因が解消したとの断定はしない。

既存proc-macro-error2 2.0.1の将来互換性警告1種類は残る。
実RPCのエラー応答・タイムアウトを意図的に起こす試験は未実施。
初回config取得失敗時に安全停止する挙動は実際に確認した。

## 次Phaseと実TXまでの条件

次の最小作業はTestnetの「署名なし取引準備・手数料照会」を既存
PreparedTransfer/pre_sign_checkへ接続すること。今回は未開始。
構築に必要なのは公開送付元・宛先、金額、valid_from、最新config prefix。
同じVersionedMessageのgetFeeForMessageで見積もりを取り、
残高、amount+fee、見積もり上限、有効期限、表示内容の一致を確認する。
過去の5,000 kelvinは現在の手数料保証ではない。

実ウォレット署名には正確なメッセージを署名できる正式な署名者の選定と、
公開鍵一致確認が必要。CLI alias defaultとの直接互換性は未確認。
本番send_transactionとsignature statuses/get_transactionによる結果確認は
まだアプリへ接続されていない。送信結果不明時は自動再送せず停止する設計が必要。
履歴消失・巻き戻しの限界も残る。

実TX試験へ進む前に、現在のネットワーク状態と利用する公開アドレス、
宛先、送金額、最新手数料、資金の有無を改めて確認する。
ウォレットアクセス・実署名・TX送信・必要ならFaucetについて、
具体的内容を提示して別途明示的なユーザー承認を受けるまで実行しない。

今回の実ウォレット操作・実署名・TX送信・Faucet・資産移動・本番状態初期化・
Git commit/push・Notion記録は0回。既存オフラインテストの合成署名は実ウォレット署名ではない。
Phase 7Lの読み取り接続検証で停止する。
