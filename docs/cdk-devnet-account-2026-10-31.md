# 2026-10-31 公開アカウント情報照会

**実装・オフライン検証PASS。DevNetオンライン検証PARTIAL。**
既存記録の公開アカウント1件を1回照会し、公式型のvalue=nullを正常なnot_foundとして処理した。
実在アカウントのOwner/Executable/Dataのlive応答は今回取得できていない。
以下の実装・初回検証の記録に、末尾の最終調査BLOCKEDとフェーズ終了判断を追記した。
「commit/pushを行わない」等は開発・検証時点の境界であり、後続の記録公開とは区別する。

## 既存機能と不足

開始時はmain、ローカルHEAD a5e8704。前日の公開完了記録はcommit
81fcda87491d11a944e5c2ac556e2e79bf167c60。元の作業ツリーを保全する分離公開だったため、
ローカルHEADとの差を未pushや成果消失とは判断しない。今回fetch/commit/pushは行わない。
既存Cargo.toml・src/lib.rs・src/monitor.rsの差分と未追跡成果物を維持する。

| 既存の根拠 | 確認内容 |
|---|---|
| src/monitor.rs:18〜31 | MonitorRpcはbalance/height/statuses。アカウント詳細APIなし |
| src/status_readonly.rs:19〜26 | 前日のTX状態照会。アカウントOwner/Dataと別の用途 |
| src/monitor/query.rs:17〜28 | 履歴とTX詳細の読み取り。アカウント詳細の再実装ではない |
| docs/cdk-devnet-status-completion-2026-10-30.md | 前日は新規9＋既存52＝61成功、CLI3条件、実DevNet状態照会PASS |
| docs/2026-10-11-testnet-transfer-e2e.md:21〜22,80 | 今回対象とする公開アドレスの過去DevNet残高観測の根拠 |

公開プログラムやアカウントの所有プログラム・実行可否・Data形式/サイズの調査に必要で、
既存の残高/TX状態機能では代替できないため追加した。

## 固定版公式SDKとの照合

導入済み公式配布ソース（版はすべて0.18.1）を根拠とする。

- rialo-cdk/src/rpc/client.rs:554〜571：get_account_info。getAccountInfoへ
  version=0、address、configのobjectを1要素paramsとして送る。署名・認証不要。
- rialo-api-types/src/messages/get_account_info.rs:73以降：GetAccountInfoRequest、
  config.dataSlice(offset,length)、AccountInfoResponse(context,value:Option)。
- rialo-shared-types/src/lib.rs:14〜21：kelvin、owner、data:Vec<String>、executable、
  rentEpoch、space。サンプルJSONのkelvins表記ではなく実際の型のkelvinを採用。
- rialo-cdk/src/rpc/types.rs:288/332：公式config/responseの再exportとAccountInfo alias。
- SDK便利メソッドはvalue=nullをRPC errorへ変換する。
  今回は同じ公式AccountInfoResponseをcall_with_json経由で復号し、nullと通信エラーを区別する。
- RPCは導入済みSDKのURL_DEVNET=http://devnet.rialo.io:4100に固定。

## 実装

| ファイル | 内容 |
|---|---|
| src/account_readonly.rs | 固定getAccountInfo adapter、公式response型、null/不正応答の区別、Data上限、メッセージを出さないエラー分類 |
| src/bin/devnet_account.rs | 前日のCLIと同じ--execute明示方式、DevNet固定、1要求・再試行なし、Data要約だけをJSON出力 |
| src/lib.rs | account_readonly公開1行だけを追加。既存status_readonly等を保持 |
| tests/account_readonly.rs | 存在/不存在、必須欠損、Data上限、encoding、RPCエラー、timeout、CLIのMock試験 |

使用例（以下のPUBLIC_KEYは実際の公開アドレスに置換）：

```sh
cargo run --offline --locked --bin devnet_account -- --address PUBLIC_KEY
```

既定は入力検証だけで接続しない。将来の明示的な接続は--executeを追加。
timeout1〜30秒、既定10秒。不正アドレス・重複option・任意RPC/送金optionは接続前に拒否する。

### Dataの境界

先頭4096byteをdataSliceで要求。base64の文字数上限を復号前に検査し、復号後も4096byteを超えたら停止。
形式が不明なDataをbase58等から推測復号しない。
Raw Dataは非Serialize/非DebugのAccountObservation内に置き、明示的なdata()でメモリ内参照だけ可能。
CLIはencoding・返却byte数・要求offset/上限だけを表示し、生Dataやそのhashは表示/保存しない。
spaceはSDKの公開フィールドとして保持し、返却slice長から全Data取得完了や切詰めの有無を断定しない。

データ上限は要求slice・復号・表示の上限。SDKのHTTP/JSON受信自体には、この追加実装による
ストリーミングbody上限はない。サーバーが巨大応答でslice制限を無視する場合の受信メモリ上限は保証しない。
CLIはSDK loggerを初期化せず、エラー出力は固定分類と数値codeのみ。生レスポンス/エラー本文は出さない。

### 不存在とエラー

明示されたvalue=nullのみnot_found。value欠損、型不正、必須フィールド欠損はmalformed_response。
ownerはSDK Pubkeyで検証。HTTP status/RPC codeは保持するが、供給元message/bodyは返さない。
分類はinvalid_input、timeout、transport、http、rpc、malformed_response、unsupported_encoding、data_limit。
不存在とRPC失敗を同じ結果にしない。再試行・追加探索・書き込みはない。

## オフライン検証

| 対象 | 成功 | 失敗 |
|---|---:|---:|
| 新規要求形式unit | 1 | 0 |
| 新規アカウント結合Mock | 12 | 0 |
| 既存status_readonly回帰 | 9 | 0 |
| 既存Monitor CLI回帰 | 12 | 0 |
| 登録テスト合計 | 34 | 0 |
| 別枠CLI smoke | 3条件 | 0 |

新規は13、既存は21。CLI smokeを登録テスト数へ加算しない。
既存phase7z_regression.block_networkを再利用し、ビルド/テスト/CLIにLinux seccompを継承。
専用HOME/TMPDIR、cargo --offline --locked、ビルド並列2。署名系・AGP・容量試験は実行しない。
既存のsource固定allowlistは変更しない。全suiteの成功とは主張しない。
新規Rustファイルのrustfmt --check、git diff --checkも成功。

## オンライン検証

対象は過去DevNet残高観測のある公開アドレス1件のみ。

```text
FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6
```

新CLIを2026-10-31に実行。getAccountInfoを1要求、再試行0、終了コード0。
結果記録時刻10:15:20.366611 UTC（19:15:20.366611 JST）。
context.slot=6096321、api_version=0.23.0-alpha.0、status=not_found、account=null。
対象を勝手に切り替えず、追加探索/照会は行っていない。

オンラインPARTIAL：実DevNetとの通信・公式null応答の処理は確認済み。
実在アカウントのOwner/Executable/Dataの返却、Dataの実encoding、サーバーのdataSlice適用は未検証。
Mockの成功を実環境の全経路の保証へ拡張しない。

## 保全と次の境界

開始時の既存ファイルの変更はsrc/lib.rsへのmodule公開1行のみ。
前日のstatusコード・テスト・記録、Cargo.toml/Cargo.lock、src/monitor.rsと旧Journalを保全する。
AGPコード/設定/serviceも開始時hashと照合する。AGP/Telegram操作・署名・送金・秘密情報アクセス、
GitHub commit/push、Notion記録は行わない。

残るのは、実在が別の根拠で確定した公開DevNetアカウントに対する正常系1件のlive照合。
今回対象の不存在結果を理由に、同じ照会や既存テストを繰り返す必要はない。
対応する公開データが得られるまではオンライン正常系を保留し、全環境動作保証をしない。
新しい機能をさらに増やす前に、この境界の確認を優先する。

集計・source/log hashとオンライン選択結果は
[検証結果JSON](cdk-devnet-account-results-2026-10-31.json)に保存する。

## 最終調査とフェーズ終了記録

### 実装済み

Owner・Executable・Dataのサイズ/形式確認、context保持、明示的nullの不存在処理、
RPC/HTTP/timeout等のエラー分類。実在の正常応答の処理は合成データで検証した範囲である。

### 検証済み

新規13＋既存回帰21＝34成功・0失敗、別枠CLI3条件成功。
DevNetへ1要求・自動再試行0回で接続し、context.slot=6096321、
api_version=0.23.0-alpha.0、value=nullをnot_foundとして正常終了した。
不存在応答のcontext取得と、実在アカウントのcontext取得の検証は別である。

### 最終オンライン調査：BLOCKED

前日のDevNet成功Tx記録と今回のPARTIAL記録を読み取り確認したが、
成功Txに関連する公開アカウントのアドレスが保存されていなかった。
前回対象はnot_foundで、実在アカウントを選ぶ根拠を満たさない。
Tx署名からアドレスを推測せず、TestnetアドレスもDevNetの存在証明へ流用していない。
対象不足により最終調査のRPC要求は0回。オンラインPARTIALの1要求と混同しない。
これは対象選定の阻害であり、正常応答を処理できなかった機能FAILではない。

### 未検証・保証外

- 実在DevNetアカウントのOwner・Executable・Data・contextの取得。
- 実サーバーでのData encodingとdataSliceの適用。
- SDKのHTTP/JSON応答全体のメモリ上限保証。4KiBの要求/復号制限とは別。
- 実サーバーの異常応答・timeout・通信障害。Mock試験は完了済み。

### 終了判断と再検証条件

**今回の開発フェーズは、未検証範囲を明示した状態で終了する。全経路PASSとはしない。**
正常系のオンライン検証は保留。今後、別の作業でDevNet由来・アドレス・実在根拠が
明示された公開アカウントが得られ、限定READ ONLY照会が許可された場合のみ再評価できる。
同じ履歴探索、不存在だった対象の再照会、成功済み試験の反復は提案しない。
新規アカウント作成・送金によって検証用アカウントを補わない。

記録公開は分離チェックアウトで今回の6ファイルだけを対象とする。
前日のstatus機能・必要設定は既存remote mainを継承し、旧Journal・AGP/Telegram・
その他の未コミット変更は含めない。元作業ツリーとAGP関連ファイルはhash照合で保全する。
記録作業では新機能追加、テスト/RPCの再実行、秘密情報アクセスを行わない。
