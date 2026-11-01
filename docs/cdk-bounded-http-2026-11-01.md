# 2026-11-01 独立READ ONLY HTTP Transport

**オフライン開発PASS。独立TransportのDevNet互換性PASS（不存在応答1件の範囲）。**
既存SDK通信を変更せず、公開アカウントCLIから明示選択する受信量制限経路を追加した。
以下のオフライン開発時点の記録に、末尾の最終オンライン検証と記録公開の結果を追記した。
「外部接続・GitHub/Notion記録をしない」「オンライン未検証」は初回オフライン開発の境界である。

## 既存制約と依存追加

rialo-cdk 0.18.1/src/rpc/client.rs:165のHTTP clientは非公開。
同:443のtext、:477のjsonは本文全体を取得し、受信上限を設定する公開APIはない。
既存src/account_readonly.rsの4KiB Data制限はHTTP受信後の制限である。

今回明示許可されたreqwest =0.12.28だけを直接依存へ追加。
default-features=false、json/rustls-tlsを指定した。これらは既存CDKが既に使うfeatureであり、
既存のblocking/http2等はfeature統合で残るが、新TransportはHTTP/1専用である。
Response::chunkは追加のstream featureなしで利用できる（reqwest async_impl/response.rs:315）。
Cargo.lockはrialo-firstの依存リストへreqwestを1行追加しただけ。
全package・version・checksum・他packageの依存は開始時と一致。新規取得・依存版更新なし。
cargo check --offline --lockedは成功。SDK配布ソースは変更していない。

## 実装・CLI

| ファイル | 役割 |
|---|---|
| src/bounded_http.rs | 固定DevNet URL、READ ONLY allowlist、HTTP制御、累積受信量検査、JSON-RPC検証 |
| src/bounded_http/tests.rs | 127.0.0.1模擬HTTPサーバー、切断確認、異常系、既存account処理との照合 |
| tests/fixtures/http/oversized.json.gz | 合成gzip fixture。圧縮1054byte、展開時のJSON1048579byte |
| src/account_readonly.rs | request_paramsをcrate内共有、新Transport用の固定エラー分類追加。Data/不存在処理は再利用 |
| src/bin/devnet_account.rs | 明示的な--bounded-http経路。既定のSDK経路は維持 |
| src/lib.rs | bounded_httpの公開 |
| Cargo.toml / Cargo.lock | 許可された既存reqwestの直接依存と依存辺だけ |

```sh
# どちらも--executeなしでは入力確認だけ。ネットワーク接続しない。
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY --bounded-http
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY --bounded-http --max-body-bytes 1048576
```

将来の明示的なライブ実行は--executeを追加するが、今回は実行していない。
--max-body-bytesは--bounded-http指定時のみ受け付ける。任意endpoint指定はない。
正常時のreport構造、Owner/Executable/Data要約/context、不存在の扱いは既存処理をそのまま利用する。

## 通信・JSON制御

- allowlistはgetAccountInfoだけ。未許可名は接続前に拒否する。汎用メソッド選択CLIなし。
- 公開constructorは公式SDKのURL_DEVNETのみ。任意URL constructorはmodule内部で、テストだけがloopback URLを使用する。
- proxy利用・redirect・retry・圧縮自動展開を無効化。Connection: close、接続pool再利用なし。
- 接続3秒、読み取り3秒、全体10秒が既定。CLIのtimeout（1〜30秒）を全体へ適用し、接続/読み取りも必要なら短縮する。
- Content-Lengthが上限を超えればアプリが本文chunkを読む前に拒否。
- Content-Lengthなしでもchunkの累積長で判定し、上限を超えるchunkはbufferへ追加しない。
- 不正なContent-Length/不正HTTP framingはHTTPライブラリまたはadapterで拒否する。値を信頼して無制限に読むfallbackはない。
- 上限内で受信完了してからJSON解析。jsonrpc=2.0、要求IDと応答IDの一致、result/errorの排他、errorの数値codeとmessage型を検査する。
- HTTP 4xx/5xxとJSON-RPC errorを区別。エラーbodyも同じ受信上限。応答本文・message/dataはログ/エラーへ出さず、固定分類・数値だけを返す。

### 圧縮応答

Accept-Encoding: identityを指定し、すべての非identity Content-Encodingを本文読取り前に拒否する。
圧縮の成功処理は提供しない。自動展開を無効にしているため、展開後に上限を検査するまで
巨大Dataを確保してしまう経路はない。1MiBを超えるJSONへ展開される有効なgzip fixtureを用いて拒否を確認した。

## 上限と根拠

HTTP本文既定1MiB、設定可能範囲1byte〜8MiB。0/無制限/8MiB超は拒否する。
1MiBは運用上の有限policyで、SDKが保証する最大正常応答サイズではない。
公式型に沿う4KiB Dataの合成JSON-RPC正常応答は5685byte（base64部分5464文字）。
1MiBには十分な余裕があるが、これは実在アカウントの実測ではない。
上限を超える正常応答が将来存在した場合も安全側で拒否する。

| 対象 | 今回の保証範囲 |
|---|---|
| HTTP本文 | adapterがHTTPライブラリから受け取る本文chunkの累積長。違反chunkのbuffer追加前に停止 |
| JSON | 本文上限を通ったbufferだけを解析。JSON木・allocator・一時値のメモリ量は本文byte数より大きくなり得る |
| Account Data | 既存4KiB要求/復号上限を維持。生Dataをreportに含めない |
| CLI | 生Data非表示、report形式を維持。プロセスRSS/出力全体の厳密なbyte上限は保証しない |

Content-Length過少申告でJSONが途中になる場合は解析エラーになる。
宣言長ちょうどの有効なJSONの後に接続末尾のbyteが付いた場合、HTTP framing上の本文は宣言長まで。
末尾byteをJSONや次の応答として解釈せず、接続も再利用しない。
虚偽の宣言長を常に検出できる、TCP上の全byteをこの本文上限で制御できる、とは主張しない。
HTTPライブラリ・OSの先読みbuffer、header、chunk自体の保持量はこの本文buffer上限とは別。
全体timeoutも同期JSON解析を強制preemptするCPU/RSS制限ではない。

## 検証結果と失敗の訂正

ローカルTransport登録テストは一意25件。最初は24成功・1失敗。
失敗は全体timeout試験のserver終了待ちで、同期thread joinがTokioのHTTP接続終了処理を
進める機会を奪っていた。timeoutは期待どおり返っていた。
試験側だけでclientをdropし、runtimeを進めて切断を待ってからjoinするよう修正。
該当1件のみ再実行して成功。成功済み24件は再実行していない。製品Transportの修正は不要だった。

| 区分 | 最終一意成功 | 未解決失敗 |
|---|---:|---:|
| 新規Transport | 25 | 0 |
| 既存account要求unit | 1 | 0 |
| 既存account回帰 | 12 | 0 |
| 既存status回帰 | 9 | 0 |
| 合計 | 47 | 0 |
| CLI smoke（別枠） | 6条件 | 0 |

登録テストの延べ実行48件＝成功47・初回失敗1。修正後の一意最終状態は47成功。
25件は正常/境界/超過/CLなし/過少/過大/不正CL/chunked/実切断、不正JSON/HTTP4xx5xx/RPC error/ID/version、
接続失敗/読み取りtimeout/全体timeout、allowlist/redirect/圧縮、存在/不存在/既存形式、設定/CLIを網羅。
chunked超過試験は64MiBの本文を逐次生成するserverを使用し、clientの上限エラーと、
全量送信前にserver側が切断を観測したことをassertした。全量取得後のサイズ判定ではない。
許可されないメソッドはserverへの接続0、redirect先への接続0を確認した。

最初のsocket作成probeはsandboxで拒否され、テスト自体は未実行だった。
loopback試験だけ実行環境の承認を得て実施。回帰/CLI/ビルドは既存seccompでネットワークを遮断。
HOME/TMPDIRは専用の一時領域。DevNetや外部RPCには接続していない。
最終cargo check --offline --locked、対象rustfmt --check、git diff --checkも成功。

## 保全・残作業

開始時のCargo.toml default-run、status/account/旧Journal等の未コミット変更を保持。
SDK配布ソース、AGP/Telegramコード・設定・serviceのhashは開始時と一致することを確認する。
署名/送金機能の追加・秘密情報アクセス・サービス操作・GitHub/Notion記録は行わない。

今回のオフライン開発条件は満たした。実DevNetの新経路互換性は未検証のまま。
今後必要なら、別途許可された限定READ ONLY照会で、新経路の正常系を1件確認する。
オンラインで実在確認できなかった前フェーズの事実や未検証範囲は変更しない。
圧縮を受け付ける機能やプロセス全体の強いメモリ上限が必要なら、別要件として設計する。

詳細は[結果JSON](cdk-bounded-http-results-2026-11-01.json)を参照。

## 最終オンライン検証・フェーズ終了

### 事前条件と数値診断

既存の47件成功記録、実行ログhashとsource hashを照合し、成功済み試験を反復しなかった。
CLIの--bounded-http、getAccountInfo限定、DevNet固定、1MiB、redirect/retry無効、timeout、
認証/署名不要、単独1要求であることをコードで再確認した。

HTTPステータスと受信量の正確な記録のため、本文を含まないHttpReceiptを追加した。
成功したJSON-RPC検証後だけ、http_status/body_bytes/body_limit/request_id/jsonrpc_versionを保持。
bounded経路のCLI出力にhttp_receiptを追加し、既存reportおよびSDK経路の出力は維持した。
この数値診断のloopback試験を1件だけ追加し、1成功・0失敗。
以前の47件と合わせ、各変更時点での一意成功は48件。全件再実行したとの主張ではない。

### 実行結果：PASS

既存記録の同じ公開アドレスだけを使用し、アドレス探索はしていない。
実行開始2026-11-01 04:57:15.793048 UTC（13:57:15.793048 JST）、終了04:57:16.732837 UTC。

| 項目 | 結果 |
|---|---|
| SDK/API型・HTTP client | rialo-cdk 0.18.1 / reqwest 0.12.28 |
| RPC | http://devnet.rialo.io:4100 |
| 使用経路 | devnet_account --bounded-http --max-body-bytes 1048576 --timeout-secs 10 --execute |
| API / RPC要求 / 自動再試行 | getAccountInfo / 1 / 0 |
| HTTP status | 200 |
| 受信本文 / 設定上限 | 106 byte / 1,048,576 byte |
| JSON-RPC | version=2.0、request/response ID=1一致、result/error排他を検証済み |
| context.slot | 6,544,974 |
| API version | 0.23.0-alpha.0 |
| アカウント | not_found、account=null |
| CLI終了コード | 0 |

1回限りのclaimを実行前に作成し、同じ照会を再実行していない。
上限の有効化は実行設定・受信loop・数値診断で確認。実DevNetへ巨大応答を要求したわけではなく、
超過時の実切断は既存loopback試験の証拠を利用した。
不存在を正常に処理できたため、今回のTransport互換性条件ではPASS。
実在アカウントのOwner/Executable/Data/contextの正常系未検証は前フェーズから維持する。

### 公開と終了の境界

記録公開は前回mainの899346fを基準とする分離チェックアウトで関連11ファイルだけを対象にする。
元の旧Journal・その他未コミット変更は含めない。依存追加は許可されたreqwestの直接依存辺だけで、
パッケージ版の変更やSDK改造はない。AGP/Telegramコード・設定・serviceはhash照合で保全する。

今回のTransport開発・限定オンライン検証は完了。NotionとGitHubの記録反映を確認してフェーズを終了する。
圧縮成功処理、プロセスRSS/同期JSON CPU上限、HTTP framing外のTCP byte制限は保証しない。
同じRPC照会・既存試験・アドレス探索の反復は不要。

オンライン結果は[選択結果JSON](cdk-bounded-http-online-2026-11-01.json)に保存する。
