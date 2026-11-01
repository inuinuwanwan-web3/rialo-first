# 2026-11-01 READ ONLY観測データの時系列比較強化

## 判定：オフライン開発PASS
既存の複数入力・分類・対象分離・時系列ソート・重複除外・比較不能判定を再利用した。必要だったのは観測間隔とslot/status/存在状態の差分の明示であり、RPC機能や別の集計器を追加しない。
開始時main、HEAD a5e8704ae884770ddc21bd98ce9ad2f3402abe57。既存の未コミット成果物を保持し、本フェーズで既存変更したのは比較モジュールと比較CLIだけ。

## 明示オプションと互換性
```sh
# 従来どおりの出力
report_analysis --json a.json b.json
# 詳細を明示選択。テキスト出力でも利用可能
report_analysis --json --temporal-details a.json b.json
```
入力は既存共通schema_version=1.0。分析出力は既存analysis_schema_version=1.0を維持し、オプション時のみtemporal_details_version=1.0と各比較のtemporalを追加する。分析結果は複数network/operationを扱うため、架空の単一RPC観測レポートへ変換しない。新オプションなしの出力は従来値と完全一致するテストを通した。

## 詳細差分
- time_basis：旧形式のtimestamp_utc_report_creation、または新メタデータのobservation_completed_at_utc。取得/生成/ブロック時刻を同一視しない。
- interval_ms：比較軸のUTCミリ秒差。既存厳格日時検証を再利用し、Gregorian暦の閏日/年境界を整数計算。依存追加なし。
- slot_change：direction=increased/decreased/unchanged/unknown、magnitude=絶対差またはnull。u64最大値の差でも符号付き整数overflowを起こさない。
- status_changed：保存された照会statusの変化だけを検出。
- account_presence_before/after：account_lookupのnot_foundはabsent、明示successとaccountオブジェクトはpresent。非アカウント操作はnot_applicable。
- query_outcome_before/after：比較可能なsuccess/not_foundはsuccessful_responseという入力の状態分類。HTTP 200そのものや独立した通信実証を意味しない。
- inference=field_differences_only。slotの増減からアカウント状態変更、Tx成功、finality、台帳連続性、reorgを推測しない。

既存changedはslotを含む保存フィールドの差であり、アカウント状態だけの変化ではない。not_found継続でslotだけ増える場合はstatus_changed=false、presence=absentのまま。

## 安全性・比較不能
network・operation・address/signature・時間軸でグループ分離。入力順は保証せず、検証済みUTC軸で昇順に並べ替える。同一時刻の異なるデータは順序を捏造せず比較不可。完全一致の正規化JSONは既存SHA-256で重複除外。
slotがnullならslot差不明。対象識別子不足、単一観測、エラー/blocked/unverifiedで対象へ紐付け不能なものは比較不可。失敗レポートに対象がない場合、前の対象の状態変化として推測しない。異なる時刻を持つ同一対象のsuccess↔not_foundは照会結果/存在状態の変化としてのみ示す。
既存の入力64件・1MiB/件・合計8MiB・深さ32・4096比較点・出力1MiB（改行別）を維持。出力超過時は部分JSONを出さない。許可項目だけを投影し、任意provider本文/生Dataを出さない。任意入力の完全匿名性やプロセスRSSの保証は引き続き対象外。

## テスト結果
新規21件成功：正常間隔、逆順、同一時刻、slot増加/減少/不明/不変、status/存在変化、not_found継続、network/operation/対象不一致、重複、不正JSON/必須欠損、任意文字列非出力、情報不足、従来出力不変、閏日/年境界、u64最大差、観測完了軸、対象不明timeoutの非帰属。
既存87件成功：比較31、観測メタデータ15、既存CLI5、完了確認2、旧インポート16、共通Report18。
**合計108件成功、失敗0件。** 今回の全件であり、過去件数は足していない。
最初の元ツリーでのビルドは既存phase8a_journalのjournalモジュール解決エラーとなり、テスト未実行。ソース内の既存pub mod journalは維持。共有成果物との不整合を切り分けるため、現在のsrc/tests/Cargoとbyte一致する分離コピーで再実行し全108件成功。無関係なソースを変更/削除して回避していない。共有キャッシュ原因の完全診断は本範囲外。
Linux seccompでネットワークsyscallを拒否し、cargo --offline --locked、CARGO_INCREMENTAL=0、並列2で実行。オフラインbuildも成功。旧直接モジュール試験から新APIを使用しないdead_code警告と既存proc-macro-error2将来互換性警告はあるが、エラーではない。

## 実データと模擬データ
新規テスト・詳細表示例はすべて合成。保存済み実共通レポート2件もオフラインで読み取り、同一対象であることを確認して公開アドレスだけを同じ合成キーへ匿名化したコピーで分析した。
過去slot6,568,878は生成時刻軸、slot6,584,270は観測完了軸なので、正しく別グループ・比較可能ペア0組となった。新しいRPCやデータ取得は行わない。結果JSONには合成の詳細例と実保存データの比較不能結果を分けて保存する。実DevNetの同じ観測軸の複数時点比較は未検証のまま。

## 変更ファイル（5件）
- src/offline_reports/mod.rs：既存分析の明示詳細モードと差分計算。
- src/bin/report_analysis.rs：--temporal-details選択。
- tests/temporal_details.rs：新規21試験。
- docs/cdk-temporal-details-results-2026-11-01.json：判定、詳細例、元データ/ソース/ログhash。
- 本書。
開始時hashで上記既存2ファイル以外のsrc/tests/docs/Cargo不変を確認。前フェーズ変更を残して追加し、SDK・HTTP Transport・AGP・Telegram・既存設定は変更しない。

## 未検証・記録候補・次フェーズ
同じ時間軸の実データペア、ホスト時計同期、最悪RSS/CPU、悪意あるファイル差し替えは未検証。記録候補はオフラインPASS、108件成功、オプション互換性、意味を分けた差分、実データ比較不能の理由。今回はGitHub/Notion書き込みを行わない。
次は実保存データが別作業で蓄積した場合の同軸比較、または既存入力上限内での性能測定。追加オンライン照会や同じ探索を自動実施しない。

## 最終検証・正式公開（2026-11-01）
依頼中のsrc/bin/temporal_analysis.rsは存在せず、実装は既存src/bin/report_analysis.rsである。別名CLIの重複開発はしない。
現行src/tests/Cargoとbyte一致する全ソース分離コピーで、新規21＋既存87＝108件成功・失敗0件を再確認。オフラインビルドも成功し、現在ソースのコンパイルエラーは認められない。元の共有target不整合の機構を完全診断したとは主張しない。
公開版は既存main3e7c7d2を基に、今回の比較モジュール/CLI/21件テスト/本書/結果JSONの5件だけを分離。観測完了時刻を安全に扱う入力検証と時間軸グループ化は比較の必要前提として含めるが、過去の観測取得CLI/インポーター/無関係な変更は含めない。インポーター向けヘルパー可視性変更は公開版へ取り込まない。テストの合成fixture値は同じ値を内包し、未公開fixtureへの依存を除去した。
公開版でも新規21＋既存56＝77件成功・失敗0件、コンパイル/オフラインbuild成功。差の31件は未公開の観測メタデータ15件・旧インポーター16件であり、現行ローカル108件側では実行済み。公開mainに108件すべてが存在するという記録ではない。両検証のログhashと公開版ソースhashを結果JSONへ保存する。
既存実共通レポート2件の元hash/同一対象/network/operationを再確認したが、生成時刻と観測完了時刻という異なる時間軸なので適合ペア0組。実データ比較はUNVERIFIED。新規観測や追加RPCは行っていない。
通常pushとNotionへの正式記録は今回の利用者指示により許可された記録操作として実施する。上記の開発時点の未公開記述は当時の履歴として保持し、本節が後続の正式公開範囲を示す。
