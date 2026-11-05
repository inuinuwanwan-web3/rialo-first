# 2026-11-05 READ ONLY Release性能検証

A. 総合判定：PARTIAL。暫定Release合成workloadの限定判定はPASS。
実業務SLO、実DevNet正常系、同じ時間軸の実観測比較は未検証であり、全体PASSへ拡張しない。

B. 既存成果の確認
GitHub main `0fcdaee9416c84b46fca534dcd6d328f2ba6a64a` は11月1日の利用実績・Release計画の正式記録。GitHub API/ls-remoteで読み取り照合し、対応JSONはローカルとbyte一致。ローカルHEADはa5e8704で、未コミット成果を含む現行コードを検証対象とした。checkout/fetch/mergeは実施していない。
指定Markdown/JSON、debug性能22条件66実行、複数時点比較、観測時刻詳細、DevNet account/statusの既存記録と実装を確認した。過去108テスト等は今回件数へ加算しない。
既存の実測はdebug合成性能および限定DevNet照会。account4照会はいずれもnot_foundで、実在account取得成功とは数えない。Tx状態1照会を継続監視実績に数えず、Testnet履歴をDevNetへ合算しない。

C. 今回の作業
最小作業は既存runnerのbinary/repeat指定と固定core選択、結果出力先の分離だった。製品コードや入力上限は変更せず、既存fixture/検証ロジックを再利用した。
追加したのは20試行指定、p95、各入力hash、候補基準の検査、出力再現性/RSS/入力とbinary不変の検査、異常時の安全な証跡保存、timeout時の自分のプロセスグループ停止、既存出力上書き拒否。4097点controlも追加した。
`cargo build --offline --locked --release --bin report_analysis` 成功、jobs=2、incremental=0、専用target `/tmp/rialo-release-next-target`、ビルド269秒。ネットワークをseccompで拒否し、キャッシュ取得は行っていない。
AS256MiB、CPU5秒、wall12秒、core0、開始MemAvailable1GiB/各trial512MiBのguardを維持。concurrency=1、独立プロセス、warm cache、cache破棄なし。GNU timeのCLI子RSSとguard/起動込みの親wallを記録。p95は20値中19番目。

D. テスト結果
core6条件×20=120成功、追加control3条件×20=60成功。runner安全性テスト3成功（最初の想定外終了で停止、既存証跡保持、Release20試行必須）。合計183検査実行成功・予期しない失敗0。プロファイルとしては9条件成功。期待exit2は機能失敗に数えない。
100/1000ファイルはfile_count_limitによる拒否測定で、処理成功性能ではない。空ファイル/空object/4097点controlには時間SLOを適用しない。

|条件|入力bytes|wall中央値ms|p95 ms|最大RSS KiB|期待結果|
|---|---:|---:|---:|---:|---|
|10_reports|2391|3.8000|4.287|2492|success|
|100_reports_rejected|23992|3.4665|4.021|2492|file_count_limit|
|1000_reports_rejected|240893|4.2800|4.460|2620|file_count_limit|
|maximum_64_files_8mib|8388608|22.2285|23.302|3132|success|
|empty_file|0|3.8990|4.344|2480|invalid_json|
|empty_json_object|2|3.7845|4.510|2488|unsupported_schema|
|dense_json_value_under_1mib|800265|33.3780|34.803|31208|success|
|4096_points_output_cap|723271|106.1635|134.394|51716|output_size_limit|
|4097_points_limit|723645|32.5640|34.074|8252|point_limit|

全coreで候補中央値と各trial RSS65536KiB以下を満たした。全条件で20/20期待結果、同条件内の出力hash一致。最大RSS51716KiB（約50.5MiB）はこのfixture集合の観測値。任意JSON構造やプロセス全体の上限保証ではない。
既存debugと共通の8条件は入力bytes・出力hashも一致。別時刻・別ビルドのため制御されたdebug/release倍率比較とは扱わない。全trial、fixture/source/binary/Cargo.lock/toolchain/ログhashは対応JSONへ保存した。

E. 変更ファイル
- scripts/cdk_readonly_performance.py
- scripts/test_cdk_readonly_performance.py（新規）
- docs/cdk-readonly-release-performance-results-2026-11-05.json（新規）
- 本書（新規）
既存baselineの変更はrunnerだけ。src/tests/Cargo/過去docsと無関係な未コミット変更はhash照合で保持した。AGP/Telegramの設定変更・停止・再起動・通知操作は0。system/user D-Busはsandboxで権限拒否のため現在の稼働状態は未確認。秘密情報は表示せず、追加DevNet RPC、署名、送金、commit/push、Notion書き込みは0。

F. 未検証項目
実業務件数・周期・同時実行・入力分布・必要応答SLO、A〜Dの実RPC性能/wire bytes、同じ観測時間軸の実データ比較、実在DevNet accountの正常系/encoding/dataSlice、最悪構造/実OOM近傍/メモリ全体保証、内部parser回数、AGP/Telegramの現在の稼働健全性。

G. 次の優先作業
既存の業務記録から用途・件数・周期・必要応答SLOを確定し、今回の合成候補条件との適合性を判断する。今回合格した同条件の再測定や入力上限緩和を優先しない。
同じ時間軸の真正な観測や実在accountの根拠が別作業で得られた場合は保存データで再評価する。追加RPCが必要な検証は今回の禁止条件下では実施しない。


## 正式記録・実運用条件の整理（2026-11-05）

今回許可された正式記録は本書、対応JSON、測定runner、安全性テストの4ファイルのみ。上記のcommit/push・Notion書き込み0は測定フェーズ当時の記録であり、今回の正式公開操作と区別する。既存mainから分離コピーを作り、元作業ツリーのindex/HEADと無関係な変更を保持する。製品コード・依存関係・設定を含めない。

### 数値の照合と訂正

依頼文の「空JSON33.378ms」は **密なJSON Value（800265バイト）33.378ms** の誤称。空ファイルは3.899msでinvalid_json、空オブジェクトは3.7845msでunsupported_schema。保存された全trialから十進数で再計算した正確な中央値は64件22.2285ms、4096点106.1635ms。過去の3桁表示には丸め差があるため、本書の表は4桁表示へ統一した。元のtrial・JSONの浮動小数点測定値は変更しない。

既存の測定180実行＋安全性3テスト＝183成功・失敗0は過去実績。重複負荷を避け、今回の正式記録で性能測定・Releaseビルドを再実行しない。保存済み全180trialの中央値/p95/RSS/終了理由/出力hashと、runner/testの元hashを照合した。元ツリーと公開コピーで安全性3テストを各1回（独立項目3、計6実行、失敗0）確認した。構文・差分検査も成功。過去183へ加算しない。

測定binaryは対応JSONのsource_sha256に示すローカルソースから作成したもの。公開mainのreport_analysis CLIはbyte一致、比較moduleの差は旧インポーター向け3関数のpub(crate)可視性のみ。公開main全体を再測定したという主張ではなく、無関係なローカル成果を同時公開しない。

### 実運用条件

|項目|確認結果|根拠と限界|
|---|---|---|
|実際の業務利用件数|UNVERIFIED|既知の開発検証はaccount4照会すべてnot_found、DevNet Tx状態1照会。業務実行数や全期間総数ではない|
|利用頻度・同時実行数・入力分布|UNVERIFIED|業務周期の記録なし。ベンチ試行回数やAGP周期を転用しない|
|必要応答時間|UNVERIFIED|50/500/1000/2000/100ms等は合成workload用の候補中央値で、業務要求ではない|
|実運用SLO|UNVERIFIED|latency percentile、可用性、エラー予算、対象負荷が未定義|
|実在DevNet accountの正常系|UNVERIFIED|実在account取得の成功記録0。Mock成功をliveのOwner/Executable/Data/encoding/dataSlice検証へ読み替えない|
|時系列比較の実データ|UNVERIFIED|既存共通2観測は生成時刻軸と観測完了軸が異なり、適合ペア0組|
|メモリ予算|PARTIAL|合成実測最大51716KiB、候補各trial65536KiBは達成。AS256MiBは測定guard。実運用/ホスト全体予算はUNVERIFIED|

根拠：docs/cdk-readonly-usage-release-plan-2026-11-01.md/.json、docs/cdk-readonly-operational-budget-2026-11-01.md、docs/cdk-temporal-details-2026-11-01.md/.json、今回の全trial JSON、既存Notionの利用実績・性能・時系列比較記録。未公開の既存証跡は同時commitしない。

既存Notion関連記録：

- [利用実績・Release計画](https://www.notion.so/40b511e32d13812598c1d67ffee8130e)
- [debug性能・メモリ](https://www.notion.so/40b511e32d1381c29ca9de3f19899ac3)
- [時系列比較](https://www.notion.so/40b511e32d1381d898d2ff03cbe32c74)

過去ページは履歴として保持し、今回のRelease測定はRialo DevNet親ページ配下の別日付記録にまとめる。

### 次フェーズの最優先作業

既存の業務記録と利用者要件から、用途・1回の件数/入力分布・周期/同時実行数・許容待ち時間・SLO・メモリ予算を定義する。不明値はUNVERIFIEDのまま残す。同条件の性能再測定・上限緩和・新規DevNet照会を自動実行しない。記録公開と読み戻し確認をもって今回の作業を終了する。
