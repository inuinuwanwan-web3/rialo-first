# 2026-11-01 READ ONLY利用実績と次回Release測定条件

## 判定：PARTIAL
証拠付きの実通信/オフライン実行は整理できたが、継続的な実運用件数・周期・必要応答時間のログは未記録。Release条件は既存合成workloadを再利用する**暫定測定条件**であり、需要を確定した実運用SLOではない。今回はReleaseビルド・性能再測定・追加RPCを行わない。
GitHub mainは91eb15622884448a32dc21fa697aa9d2f799c8de（前フェーズPARTIAL正式記録）と読み取り照合済み。既存の運用予算案、性能結果、runner、実装/過去ログを参照し、上限と性能測定を再実装しない。

## 1. 調査範囲と数え方
対象は既知のCDK検証証跡。複製・要約された同一観測を別実行へ加算しない。検証用実通信、合成性能試験、Mock/単体試験、継続的な実運用を区別する。以下は確認した証跡集合での回数であり、システムの全期間総数ではない。
業務/本番実行は未記録であり「実運用が一度もない」とは断定しない。Testnetの実績をDevNetへ移さず、Tx状態CLIを継続監視サービスとして数えない。AGP/Telegram運用からCDKの頻度を流用しない。

## 2. 機能別に確認した実績
|機能|確認回数/1回の件数|成功/失敗の意味|入力/出力JSONサイズ|時刻・間隔・応答時間|
|---|---|---|---|---|
|A 公開account/残高|DevNet検証4照会、各1address|RPC正常not_found4、記録上の予期しない失敗0。実在account残高の取得成功0|要求wireサイズ未記録。HTTP本文106Bが3照会で記録、最初のSDK照会サイズ未記録。CLI stdoutの元バイト数未記録|最新観測区間1037ms、外側capture1044.029ms。定期実行間隔・必要応答時間は未記録|
|B Tx状態|DevNet getSignatureStatuses検証1回、1署名|照会成功1、記録上失敗0、executed=true/err=nullのTx実行証拠1|要求/応答wire JSONサイズ未記録|started_utcはあるがstatus個別の開始/終了は未記録。応答時間/周期/SLOは未記録|
|C Tx履歴|DevNetアドレス履歴getSignaturesForAddressの実行は未記録。参考Testnetは別表|DevNet成功/失敗件数は未記録|wire JSONサイズ未記録|DevNet応答時間/実行間隔/必要応答時間は未記録|
|D DevNet状態監視|継続サービス実行回数は未記録。既存単発Tx状態はBと同じ実行|Bを別RPCとして二重計上しない|独立した監視入出力は未記録|監視周期/監視対象/応答SLOは未記録|
|E 集計・比較|性能測定22条件×3＝66 CLI実行。別の受入19ケースも記録あり|性能66実行はexit0が21、想定exit2が45、予期しない失敗0。これは開発検証で実運用ではない|10件2391B→8916B、64件8388608B→58866B等、下表参照|連続ベンチ試行は業務実行間隔ではない。必要応答時間/実運用頻度は未記録|

Aの4件：2026-10-31 SDK、11-01 bounded HTTP、同日共通JSON、同日観測メタデータ。元address一致を確認したが再掲しない。どれもnot_foundで、残高値が取得できたとは扱わない。
最新Aの開始07:07:46.279Z→完了07:07:47.316Zはローカル時計で採取した観測処理区間（クライアント準備・解析を含み、純粋なネットワーク時間ではない）。過去bounded captureは0.939789秒の外側記録区間で、同じ測定軸として直接比較しない。
共通レポート生成時刻05:59:48.109Z→07:07:47.316Zの差は4079.207秒。ただし生成時刻差であり、観測周期・業務頻度へ読み替えない。正確な同軸の観測開始は1件のみで、反復間隔は未記録。

### 別ネットワーク/別操作の参考実績
- Testnetのstatusバッチ1回・5署名が保存されているが、BのDevNet1回へ合算しない。
- Testnet履歴は既知の2検証記録で各2呼出、計4呼出。1記録は返却2/2行、もう1記録は2/0行（batch2ページ）。同じTxの重複や全期間総数は別問題であり、返却4行等をユニークな履歴総数とは扱わない。
- bounded history batchの記録開始/終了は整数秒で差2秒。秒単位の外側captureで、精密なRPC応答時間ではない。差0秒のページも0ms latencyとは扱わない。
- 2026-10-30 DevNet getTransactions(limit=1)を1回、1Tx取得した記録がある。これはTx候補探索の一覧取得で、アドレス履歴getSignaturesForAddressや常時履歴収集の実績にはしない。

### 根拠となるファイル・行
|証拠|根拠|
|---|---|
|A SDK/1address/not_found|docs/cdk-devnet-account-results-2026-10-31.json:91（online）、103（address）、109（status）|
|A bounded HTTP/1request/106B|docs/cdk-bounded-http-online-2026-11-01.json:2（開始）、6（試行）、29（HTTP receipt）|
|A共通JSON原観測|既存一時証跡のreportとdocs/cdk-readonly-json-online-2026-11-01.json:6（操作/回数）。複製を加算しない|
|A観測メタデータ|docs/cdk-observation-online-evidence-2026-11-01.json:5（回数）、11（106B）、24（開始/終了）|
|B DevNet statusと別getTransactions|docs/cdk-devnet-status-online-2026-10-30.json:8、12、24|
|C Testnet batch|docs/phase-7z-bounded-history-results.json:7（時刻）、9（calls）、82（pages_read）|
|B/C Testnet補助|docs/phase-7z-live-testnet-results.json のcalls/status/histories。src/monitor/query.rs:413は別の過去空履歴をMock再生する試験で、追加実通信ではない|
|D 単発仕様|src/monitor.rs:1、src/bin/devnet_status.rs:54。周期実行の定義はない|
|E性能|docs/cdk-readonly-performance-results-2026-11-01.json:30（cases）、66（10件中央値）、814（dense）、952（4096点）|
|E受入|docs/cdk-offline-acceptance-results-2026-11-01.json のcases（19件）。今回は再実行しない|

## 3. 証拠のあるオフライン実測値
|条件（debug・各3回）|入力|出力|wall中央値|最大RSS|
|---|---|---:|---:|---:|
|10件|10files/2391B|8916B|5.461ms|3212KiB|
|64件padding|64files/8MiB|58866B|127.505ms|3892KiB|
|密なValue|1file/800265B|753B|188.593ms|31972KiB|
|4096比較点ちょうど|16files/723271B|固定エラー60B|613.623ms|52328KiB|
|空ファイル|1file/0B|invalid_json 55B|3.989ms|3072KiB|
|空オブジェクト|1file/2B|unsupported_schema 61B|3.458ms|3060KiB|

上記は合成データによる開発測定（分類1「実測済み」）。正確な時系列・同一対象分離等のテストは分類2「オフラインテストのみ」であり、実運用SLO/Live DevNet負荷の測定ではない。51.1MiBは全測定の最大観測値で、製品全体上限ではない。

## 4. 前回の提案値との照合・分類
|今回提示された項目|分類|正しい対応と注意|
|---|---|---|
|10件50ms以内|3 提案値のみ|前回の暫定目標。debug5.461ms実測はあるがreleaseや実運用の達成証拠ではない|
|64件8MiB500ms以内|3 提案値のみ|debug127.505msはpadding fixtureでの実測。任意の最密8MiBへ一般化しない|
|空JSON1秒以内|4 根拠不足|原案1秒は空JSONではなく密なValue800265B用。空ファイルと空objectも別の拒否条件|
|4096件超2秒以内|4 根拠不足|原案2秒は4096比較点ちょうどでoutput_size_limit拒否。4096超はpoint_limitの別条件。「レポート4096件」は64file上限違反|
|RSS64MiB|3 提案値のみ|実測最大52328KiBに約25%余裕を置いた候補。常設/全プロセス保証でもrelease実測でもない|

根拠：docs/cdk-readonly-operational-budget-2026-11-01.md:65〜73。原案は変更せず、この差異を新記録で明示する。性能差の推定・release達成予測は行わない。

## 5. 次回Release測定条件（暫定・未実行）
業務SLOは未記録なので、実績があるEの固定合成workloadだけを再測定する。A〜DのRPC性能目標をこの結果で代用しない。

|core profile|入力|20試行で確認する期待結果|候補wall中央値|
|---|---|---|---:|
|10件|10files/2391B|10採用・9ペア・間隔1000ms|50ms以下|
|64件|64files/8MiB、各1MiB以下|64採用・63ペア。既存padding fixture|500ms以下|
|密なValue|1file/800265B|正常処理|1000ms以下|
|4096点|16files/723271B|output_size_limit、完全な固定JSONのみ|2000ms以下|
|100files|23992B|file_count_limit、正常処理とは数えない|100ms以下|
|1000files|240893B|file_count_limit、正常処理とは数えない|100ms以下|

- concurrency=1、releaseのoffline/locked build。source/binary/Cargo.lock/toolchain/環境を保存。必要キャッシュがなければネットワーク取得せずBLOCKED。
- 6条件×20＝120独立プロセスを計画。今回未実行。中央値/全trial、p95（20値の19番目）、GNU timeの子RSS KiB、入出力bytes/終了理由/出力hashを記録。
- wallには起動/guardを含める。Python親/timeやホスト全体のメモリをCLI RSSへ足したと主張しない。warm cacheで実施し、強制cache破棄なし。
- 既存測定guard（scripts/cdk_readonly_performance.py:19、34、63）を維持：AS256MiB、CPU5秒、wall12秒、初期MemAvailable1GiB/段階512MiB、core0、seccompネットワーク拒否。
- 120×12秒＝24分は停止timeoutの合計予算（build別）で、実所要時間の見積もりではない。最初の異常で止める。
- 候補RSS閾値64MiBは各trialで判定。期待結果20/20一致・決定的出力・候補中央値/RSS内を限定PASS条件とする。未実施/未確定業務SLOが残る全体評価はPARTIALと区別。
- signal/panic、想定外終了、誤比較、秘密本文出力、空きメモリ低下、timeout超過では追加負荷を停止し、原因を安全な証跡へ記録。リトライ/上限緩和/新規RPCで通さない。

追加の正確性controlは、空file0B→invalid_json、空object2B→unsupported_schema、**4097比較点/17files→point_limit**をcoreと別枠で計画する。4097点のJSON実bytesはfixtureを固定するまで未記録。これらの時間SLOは未設定で、原案のdense1秒/4096点2秒へ混ぜない。
現runnerはdebug/3試行固定であるため、次段階で必要ならbinary/repeat指定だけを最小追加する。今回は既存コード/runnerを変更していない。

## 6. 今回の確認・テスト・保全
新規RPC0、release/負荷測定0。既存ソース/性能記録hashと最新GitHub commitを照合。
必要箇所に限定した既存試験4件成功・失敗0：旧出力互換、新旧時間軸分離、明示ページング、RPCエラー/timeout。現行src/tests/Cargoと一致する分離コピーでコンパイルし、seccompネットワーク禁止・cargo --offline --lockedで実行した。前回63件/過去108件・性能66実行は再実行/今回件数へ加算しない。
追加は本書と結果JSONの2件のみ。既存src/tests/docs/scripts/Cargo、未コミット変更、AGP/Telegram関連を保持。設定変更・commit/push・Notion書き込みなし。

## 未記録・次の優先作業
実運用件数、周期、同時実行数、入力サイズ分布、必要応答SLO、A〜Dのwireバイト数/詳細latency、release目標達成、実メモリ枯渇/最悪構造は未記録/未確認。
次は暫定E測定が意思決定に役立つことを確認し、最小runnerパラメーター化→offline release測定。別途、実業務で必要な用途/件数/周期/SLOを確定する。100/1000件の需要も処理能力も推測せず、現上限を維持する。
