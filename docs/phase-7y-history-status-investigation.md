# Phase 7Y — Transaction History & Status Investigation

## 最終記録（2026-10-22）

総合判定：**PARTIAL**。

- TX詳細照会：現在の公式RPCでは取得できず。過去の成功TX候補2件も現在参照不可。
- TX状態：Unknown、status_reason：UnexecutedOrNotFound。
- アドレス別履歴API：正常応答、0件。非空履歴と実データのページネーションは未検証。
- Finality：未対応。履歴保持範囲とTestnetリセットの有無：未確認。
- 実装済み：状態理由表示、TX詳細取得API、1ページ履歴取得、cursor/件数検証、重複処理。
- ライブ確認済み：接続、詳細APIのNot Foundエラー、状態と理由の整合、空履歴の正常復号。
  TX詳細本文の正常取得・実行成功、非空履歴、2ページ取得は確認できていない。
- オフライン結果は段階別に記録：初期56成功・0失敗・119未実行、
  ライブ後52成功・0失敗・125未実行、最終example変更後1成功・0失敗・標準177件未実行。
  これらを重複加算せず、既存135件全体の回帰完了とは扱わない。

最後のライブ検証は21:30:58〜21:30:59 JST。以下は各段階の履歴であり、
前半のオフラインのみという記述はその時点の範囲を示す。
今回のGitHub記録作業では追加RPC・署名・送金・Faucet・Notion操作を行わない。

## 結果（2026-10-22）

総合PARTIAL。実装部分のオフライン検証はPASS。
Unknownのクライアント側の原因は特定したが、現ノードで未実行応答を返す根因は未特定。
公式配布API型に沿ったTX詳細・アドレス履歴1ページ取得をライブラリに追加。
履歴の現ノード互換性、保持範囲、未発見/未確認の識別、確認数/Finalityは未確認。
実RPC・外部通信・秘密情報読取・署名・送信・Faucet・commit/push・Notion操作なし。
末尾の記録依頼よりも今回の明示的な禁止を維持し、GitHub/Notionへは書き込まない。

開始時main、HEAD 98951af（Phase 7X）。既存未追跡
`docs/phase-7-signing-investigation.md`は読取・変更とも行わず保持。
Cargo.toml:7、Cargo.lock:3204のrialo-cdkは0.18.1。依存追加・変更なし。
Phase 7X実装、CLI、記録を確認し、既知の調査結果は再利用した。

## ローカル証拠とUnknown

以下の基点は `/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`。
C = rialo-cdk-0.18.1/src/rpc/client.rs、
T = rialo-cdk-0.18.1/src/rpc/types.rs、
A = rialo-api-types-0.18.1/src/messages/、
R = rialo-cdk-0.18.1/docs/rpc-reference.md。
公式配布のSDK実装・API型・参照文書はローカルに存在する。
現ノードのサーバー実装・公式実応答fixtureは今回確認できていない。

| 調査項目 | 関数・型・根拠 |
| --- | --- |
| TX Hashによる詳細照会 | C:586 RpcClient::get_transaction、getTransaction。入力はSignature |
| TX検索結果の型 | A/get_transaction.rs:12 GetTransactionResponse、block_height/block_time/transaction/meta。T:388 TransactionResponseはSDK互換型 |
| 状態照会 | C:619 get_signature_statuses、getSignatureStatuses |
| 状態のAPI型 | A/get_signature_statuses.rs:41 GetSignatureStatusesResponse、:66 GetIndividualSignatureStatus。valueは非Option要素のVec、slot/err/executed |
| SDK状態変換 | C:633〜645 全要素をSome化、個別slotをcontext.slotに置換、errを文字列化 |
| 未発見 | R:280はfoundならSomeと説明するが、CはNoneを生成しない。API型はnull要素を復号できない。現サーバーの未発見応答は不明 |
| 未確認 | executed=falseは表現可能。しかし処理待ち/未発見を区別するフィールド・保証は確認できない |
| 成功 | errなし/ executed=trueを実行成功として扱う。Finalityとは別 |
| 失敗 | errありを失敗として扱う。errは公式TransactionError型（API型:73） |
| RPC/HTTP/JSONエラー | C:410 GenericRpcCaller::call、:441 HTTP非成功、:477 errorオブジェクト、:501 result欠損、:508 復号失敗でErr |
| 詳細未発見 | A/get_transaction.rs:12はOptionではない。nullは型不整合。未発見時のnull/エラーコード規約はローカル資料で確認不能 |

Phase 7Xの実観測はSDK復号成功でUnknown。Cの変換とMonitorの分岐から、
その時点ではSomeかつerrなし/ executed=falseに該当することが証拠に基づく説明。
raw応答が保存されていないため個別slot等の復元はできない。
ノードがfalseを返した理由は、未発見、保持範囲外、Testnetリセット、処理待ちなどを
区別する証拠がない。これらは仮説であり、原因確定として採用しない。
SDKのslot置換は確認数に使えない理由だが、executed=falseを生成する直接原因ではない。
RPCエラーをUnknownや残高0へ変換しているわけでもない。

今回status_reasonを追加：ExecutedWithoutError、RpcReportedError、
NoStatusReturned、UnexecutedOrNotFound。既存execution値とCLI分類は変更しない。
NoStatusReturnedは注入Mock等のNone用で、現在のHTTP adapterでNoneが観測可能という意味ではない。
UnconfirmedやNotFoundを推測で新たに出力しない。

## 履歴API対応状況

分類はSDK/配布RPCスキーマの存在と、現ノードでの検証を分ける。
Bはローカル公式RPC定義に基づく利用可能な呼出経路であり、ライブ成功の認定ではない。

| 機能 | 分類 | 根拠・制約 |
| --- | --- | --- |
| アドレス指定TX検索 | B（SDKメソッド自体はAだが形式不整合） | C:727 get_signatures_for_addressはVecへ直接復号。一方A/get_signatures_for_address.rs:71はcontext/value。C:381 call_with_jsonで正式応答型を復号する経路を今回追加 |
| TX Hash詳細 | A/B | C:586 get_transaction、A/get_transaction.rs:12。今回はcontextと元のerr情報を保持するため同じHTTP transportのcall_with_jsonと元のAPI型を使用 |
| ブロック指定TX取得 | A | C:1129 get_block、:1155でgetBlock。transaction_details=Full/Signatures/None。今回は調査のみで新規実装・試験なし |
| 履歴ページング | B | A/get_signatures_for_address.rs:32 GetSignaturesForAddressConfig、limit/before/until。R:263も記載 |
| 件数制限 | B | 同:33 1〜1000、既定20。rialo-api-types-0.18.1/src/validation.rs:223 validate_signature_limitが1〜1000検証 |
| 履歴保持範囲 | C | Rはrecentと説明するが期間・pruning・archive保証なし。完全履歴と認定しない |
| 全体TX一覧 | A | C:813 get_transactions、A/get_transactions.rs:18/32/81。limit既定100、1〜1000、before。アドレス検索とは別。今回未実装 |

D（未対応の確定）は付けない。仕様不明を未対応の証明として扱わない。
R:280の状態説明やR:214以降の詳細の例は実際のフィールド名/型と一致しない箇所があり、
実装・API型を優先する。Rはノード実装の証明ではない。

## Finality・確認数

- TX詳細のblock_height、transaction、metaはブロック包含と実行結果を照会する材料。
  get_blockのTX/署名一覧も照合材料だが、今回は実照合していない。
- R:132はget_block_heightを「current finalized block height」と記述。
  A/get_signatures_for_address.rs:38にもhighest max finalized blockへの言及がある。
- ただし合意形成上のFinality定義、巻き戻り条件、確定の閾値、個別TXへの適用規約は
  今回のローカル資料では確認できない。用語の存在と定義の検証を分ける。
- 状態slotはC:638でcontext.slotへ上書きされ、個別TXの高さとして使えない。
  A/get_transaction.rs:17も「slot」と説明しつつblock_heightという名前を使う。
- 別々のRPCの高さ差は、同一chain/config・包含維持・高さとslotの意味が保証されなければ
  正式な確認数とはいえない。差分計算、+1、閾値によるFinality判定を追加しない。
- 確認数/Finalityは引き続き未対応・非表示。Unknownを補完しない。

## 実装範囲と互換性

変更はsrc/monitor.rs、新規src/monitor/query.rs、本書のみ。
既存src/main.rs、src/monitor/cli.rs、src/lib.rs、example、署名/送金準備/ライフサイクルは変更なし。
既存MonitorRpc・snapshotの引数と既存CLIの引数/終了処理を維持。
transactionsにstatus_reasonを追加するJSON拡張。RustでTransactionObservationを
直接struct literal構築する外部利用者は新フィールドの指定が必要だが、リポジトリ内の
既存呼出は影響なし。既存executionの意味を変えない。

新規ライブラリAPI：

- monitor::query::history_page(rpc, address, config, timeout)
- monitor::query::transaction_detail(rpc, tx_id, timeout)

QueryRpcはHistory/Detailの2種類のみ。実adapterは既存HttpRpcClient::call_with_jsonへ委譲。
架空RPC名・新HTTP transport・新依存を追加しない。
履歴requestには公式GetSignaturesForAddressRequest/Configを使用。
応答は公式GetSignaturesForAddressResponseのみ受理。bare配列へ推測でfallbackしない。
API型と違う応答はエラーにし、空履歴へ変換しない。

履歴は1回1ページ、入力アドレス/limit/cursorを通信前検証、timeout、件数超過拒否。
context_slot、entries、duplicates_removed、next_beforeを返す。
同一ページの重複は内容一致なら除外、矛盾ならエラー。ページ間の重複集約は未実装。
next_beforeは元の応答件数がlimitと同じ場合の最後のIDであり、次ページ存在の保証ではない。
同じbeforeへ進まない応答は拒否。自動追跡をせず、利用者が次ページを明示要求する。
少数件/空ページでも全期間の完全性は主張しない。untilは公式requestへ渡す。

詳細は公式GetTransactionResponseを返し、先頭署名と照会IDの一致を検証。
null/欠損/型不整合/RPCエラーは取得エラーで、NotFoundや成功へ自動変換しない。
新ライブラリAPIは明示的に呼ぶ場合のみ追加照会する。既存CLIから履歴/詳細を取得する
新オプションは今回追加していない。既存snapshotのaddress_history_supported=falseは
そのsnapshotが履歴を取得しないことを表すため維持した。

## テスト安全性・結果

7U/7W/7Xの安全監査を再利用。対象の既存41件の経路は変更なし。
新規QueryRpc MockはRefCell上のJSON fixtureと要求記録だけを使用。
公式API型に合う手作り合成fixtureであり、実ノード由来fixtureとは呼ばない。
固定Signatureは公開識別子形式の入力で、鍵生成・署名計算はない。
timeoutはpending future、RPCエラーは固定Err。HTTP adapterをテストから起動しない。
既存binaryテストはparse/未署名message等のみでmain/runは呼ばない。
exampleはcargo checkでコンパイル確認のみ。実行していない。
既存署名fixtureを含む119件は実行しない。秘密ファイルや親の認証環境は参照しない。

専用TMPDIRとPATH/TMPDIR/CARGO_NET_OFFLINE/LANG/RUSTUP_HOME/CARGO_HOMEの
許可リスト環境で、cargo test --offline --lockedを対象指定して実施。
Cargo offlineだけでRPCを遮断するのではなく、無通信の根拠はMockの呼出経路。

- Monitor：39成功（Phase 7Xの24＋新規15）、0失敗。
- binary：既存11成功。
- unsigned_tests：既存5成功。
- FeeCase：既存1成功。
- 合計56成功・0失敗・119未実行。列挙175件（library164＋binary11）、doctest0。
- 元の135件と7Xの25件を維持。元の135件のうち16件実行・119件未実行。
- 新規15件：履歴・詳細を含むqueryテスト14件＋状態理由テスト1件。
  空履歴、複数TX、2ページの明示要求、重複、矛盾重複、非進行cursor、不正応答形式/件数、
  不正入力、RPCエラー/timeout、詳細成功/失敗/ID不一致/null、Finality非表示を検証。
- 真の未発見/未確認の独立判定は未実装。None/falseをUnknownのまま維持する試験であり、
  ノードがNotFound/Unconfirmedを区別できることのテスト成功とは扱わない。
- cargo check --offline --locked、および--examples成功。
- cargo fmt --check、git diff --check成功。既存proc-macro-error2将来互換性警告あり。
- ログ：/tmp/rialo-7y-audit-0dpcxoim/。

## 残課題と次の作業

1. 最優先は正式なノード仕様/fixtureで未発見・未確認の区別と履歴応答形式を確認すること。
   今回外部アクセスは行わない。将来実応答採取が必要な接続先は既存公式
   https://testnet.rialo.io/ のgetSignatureStatuses/getTransaction/getSignaturesForAddress。
   この段階には別の読み取り通信許可が必要。秘密情報や署名は不要。
2. 保持範囲、before/untilの境界・順序保証とFinalityの定義を公式資料で確認する。
   ローカル資料だけでは不明なため、確認数/Finalityは保留のまま。
3. 型付きMock成功と現ノード互換性を区別したうえで、履歴/詳細のCLIオプション、
   観測時刻、複数ページ集約を追加する。今回の新規APIはライブラリのみ。

実装した機能はオフラインPASS。重点3課題には公式仕様/実応答の不足が残るため
Phase 7Y総合判定はPARTIAL。Mock成功を実Testnet成功とは扱わない。

## Live Testnet History & Status Verification（2026-10-22）

### 承認・呼出確認・実行

ユーザーの公式Testnet読み取り承認に従い、前節の根拠を再利用。
C:586 getTransaction、C:619 getSignatureStatuses、C:727 getSignaturesForAddressの
request生成とAPI型を確認し、読み取り専用と判断した3メソッドだけを実行した。
汎用call_with_jsonはC:381で既存transportへ委譲される。
API型はA/get_transaction.rs:12、A/get_signatures_for_address.rs:71。
未発見・ページング・保持範囲のローカル確認結果は前節の表を継承し、
現ノードの観測で確認できた点だけを以下に追加する。

手動用examples/phase_7y_readonly.rsを追加し、既存query関数を呼ぶ。
既存snapshotの分類処理をobserve_statusへ抽出し、ライブ検証でも同じ判定を再利用。
新しい判定ロジックやHTTP transportは作成していない。
例は--live-readonly必須、全体45秒、各要求10秒、履歴limit=2・最大2ページ。
実行コマンド：

```sh
cargo build --offline --locked --example phase_7y_readonly
./target/debug/examples/phase_7y_readonly --live-readonly
```

Phase 7Xのsandbox DNS失敗/制限外成功を参照し、同じ失敗を再試行せず、
上記コマンドと通信内容を提示して制限外実行の承認を取得した。
PATH/LANGのみの環境で起動。外側プロセスtimeout50秒。
接続先は既存 https://testnet.rialo.io/ のみ。ネットワーク設定変更なし。
開始12:18:59.284776 UTC / 21:18:59.284776 JST、
終了12:19:01.082412 UTC / 21:19:01.082412 JST。
結果保存先：/tmp/rialo-7y-live-a2d1l21x/result.json。
SDKのJSON-RPC result payloadとSDK変換後statusを記録し、HTTP全体のraw応答は未保存。
RPCエラーはSDK文字列表現を記録。数値エラーコードは今回保存していない。

公開アドレス：7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa

既知TX：5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs

### 実測値

| 項目 | 結果・判定 |
| --- | --- |
| DNS/HTTPS接続 | 接続成功。今回DNS/TLS/timeoutエラーなし |
| TX詳細 | 21:19:00 JST、RPC Error: Transaction not found for signature:（上記ID）。詳細本文は未取得 |
| TX状態 | 21:19:00 JST、SDK変換後slot=1197107、executed=false、err=null |
| Monitor分類 | execution=Unknown、status_reason=UnexecutedOrNotFound。SDK応答と整合 |
| 履歴1ページ目 | 21:19:01 JST、context.slot=1197108、value=[]。正常取得、0件 |
| API version | 履歴context.api_version=0.23.0-alpha.0（観測値。CDK版と同一とは扱わない） |
| 2ページ目 | NOT TESTED。next_beforeなしのため追加照会せず |
| 重複 | 取得範囲でページ内0・ページ間0。ただし空集合のため非空履歴での重複処理はライブ未検証 |
| Finality | 未対応。今回も推測しない |

合計3要求（詳細1、状態1、履歴1）。リトライ・追加のTX作成・追加アドレス照会なし。
exampleは各操作の結果をまとめる診断用で終了0、stderrなしだが、
この終了コードを全操作成功の根拠にはしない。detail.errorを個別に失敗として扱う。

### 状態の解釈と整合性

- 詳細APIではサーバーが明示的にTransaction not foundと応答した。今回の照会で
  見つからなかったことは確認したが、TXが過去にも存在しなかった証明ではない。
- 状態APIだけを見るとfalse/nullで、Success/Failure/Pending/Not Foundの区別はできない。
  Unknownを維持。別APIのnot foundという文字列だけで汎用の分類規則を追加しない。
- 診断上は「詳細検索でNot Found、状態検索ではUnknown、詳細取得処理はRPC Error」。
  実行失敗Failureやmempool待機Pendingとは認定しない。
- SDK変換後slotはcontext.slot由来であり、1197107をTX包含高と説明しない。
- 履歴は予想したcontext/valueの正常応答で、今回の配布API型で復号成功。
  ただし非空のSignatureInfo互換性・cursor境界・順序・最大件数はライブ未検証。
- 旧TXを含む過去成功記録と現状のNot Found/空履歴の差は存在するが、保持範囲外、
  リセット、pruning等の根因は未特定。履歴保持期間の公式保証は引き続き未確認。
- 接続障害ではなく、詳細APIのアプリケーション層RPCエラーを観測した。
  型不整合や履歴実装の不具合は今回の空履歴応答では観測していない。

### 最小変更とオフライン検証

今回の追加変更：src/monitor.rs（observe_status抽出と記録statusのfixture試験）、
src/monitor/query.rs（記録した空履歴payloadのfixture試験）、
examples/phase_7y_readonly.rs、本書。
既存署名/送金コード・依存関係・CLI引数は変更なし。
API処理に修正を要する不整合は今回観測せず、推測のfallbackや自動再試行は追加しない。

ライブで得たcontext/api_version/value空配列の形を固定fixture化し、
SDK変換後false/nullのstatusを固定fixture化してUnknown維持を検査した。
2件とも通信なし、鍵生成/署名なし。タイムアウト試験は既存Mockを再利用。
新規exampleは手動起動のみで通常のテストから通信しない。

専用TMPDIR・許可リスト環境で安全な変更対象/互換性テストだけを実行。

- cargo test --offline --locked --lib monitor::：41成功・0失敗。
- cargo test --offline --locked --bin rialo-first：11成功・0失敗。
- 今回合計52成功・0失敗・125未実行（全構成177件、library166＋binary11）。
- 今回新規2件。Phase 7Y累計新規17件、元の135＋7Xの25件を維持。
- 残り125件には既存署名fixtureがあるため全件実行していない。未実行を成功扱いしない。
- ログ：/tmp/rialo-7y-live-check-g5qhkm0_/。
- cargo fmt --check、git diff --check成功。exampleのoffline build成功。
- 既存proc-macro-error2将来互換性警告あり。

総合判定PARTIAL。履歴の空結果取得と状態/理由の整合は確認したが、
TX詳細の正常取得、非空履歴、2ページ取得、保持範囲・Finalityに未確認事項が残る。
次は現ネットワークの有効な既知TXまたは非空履歴の正式な公開fixtureを得て、
別途許可された読み取りで詳細/ページングを確認すること。新規TX作成で補わない。
その後に履歴・詳細のCLI統合、取得時刻と取得不能理由の表示を検討する。
本作業で秘密情報読取、署名、TX作成/送信、Faucet、ウォレット接続、非公式接続、
commit/push、GitHub/Notion書込は行っていない。

## Valid Transaction & History Verification（2026-10-22）

### 候補の公開記録調査

docs/（無関係な未追跡署名調査文書を除く）、examples/、README.md、PREPARATION.mdを
公開TX ID/成功記録/ネットワークに絞って確認。トップレベルtests/は存在しない。
src内の関連テストにあるmock-tx-idやSignature::fromによる固定IDはDと分類し、
実在TX候補へ混ぜない。鍵ファイル・認証情報・秘密鍵/seedの検索は行っていない。

| 分類 | TX・日時・ネットワーク・根拠 |
| --- | --- |
| A | 5TiohMeL3ryJJPQKqJ7Dxr7oUAj2ZNnu9qRv1m3Sws6omMQDT6x9vvUtNbthVubd1Nvdeb6dkGdMJ6uGPHC8myz7。2026-10-11 Testnet。docs/2026-10-11-testnet-transfer-e2e.md:45/50/52、executed=true/err=null、execution slot 31312642。正確な実行時刻は未記録 |
| A | 5o9vrT8LrU3U6Gith4FxCRzHTrHNCm5xJ9AWQCQB8A9yxaXgu1oWP4xdVJhmPD2P8oRB4oouY1StAjSnWkQKmFG8。同日Testnet。同文書:46/54/56、executed=true/err=null、execution slot 31317728。正確な実行時刻は未記録 |
| A | 5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs。同日11:04:39〜41 UTCの確認記録。同文書:87/92/94、実行成功、block height 31319418。前回Not Foundのため今回再照会しない |
| B | 今回の公開記録範囲で成功TX IDは確認できず。DevNet残高観測/準備記録はあるが、TX IDを推測しない |
| C | 今回新たに採用した候補なし |
| D | src/pre_sign/lifecycle/persistent/tests/integration.rs:59のmock-tx-id、query testsの固定Signature等。実RPCに使用しない |

上記Aは「過去の成功記録がある」という分類であり、現RPCでの存在保証ではない。
資金受取TX2件の関連公開アドレスは7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa。
履歴は前回の同アドレス空履歴を繰り返さず、別の既存受取先
FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6を対象とした。
このアドレスは同文書:78/94/104〜107に2026-10-11の送金受取・残高増加の証拠がある。
今回の資金受取TX2件の受取先だとは主張しない。

### 実行内容・安全範囲

既存手動exampleに--historical-candidatesを追加して既存query/observe_statusを再利用。
選定は固定の公開記録2件のみ。getTransaction×2、getSignatureStatuses一括×1、
getSignaturesForAddress limit=2×最大2ページ。最大5要求、各10秒、全体55秒。
通信先は既存 https://testnet.rialo.io/ のみ。
SDK根拠C:586/619/727と前回の読み取り監査を再利用し、新たなRPC名は追加しない。

```sh
cargo build --offline --locked --example phase_7y_readonly
./target/debug/examples/phase_7y_readonly --live-readonly --historical-candidates
```

実行コマンドと通信内容を示し、制限外実行の承認を取得して1回実行。
外側60秒timeout、PATH/LANGのみの環境。DNS設定変更なし、DevNet通信なし。
開始2026-10-22 12:30:58.034872 UTC / 21:30:58.034872 JST、
終了12:30:59.889026 UTC / 21:30:59.889026 JST。
記録：/tmp/rialo-7y-candidates-nf3l3ccb/result.json。

### 実測結果

- 資金受取TX2件とも詳細APIはTransaction not found for signatureのRPCエラー。
  現RPCで参照できないことを確認。詳細本文・包含block height・関連アドレスの再取得は未達。
- 状態は2件ともexecuted=false、err=null、SDK変換後slot=1202001。
  execution=Unknown、status_reason=UnexecutedOrNotFound。Success/Failure/Pendingとは分類しない。
- 詳細照会のNot Foundと状態照会のUnknownは別々の観測。
  Not Foundは現在の検索結果であり、過去の成功証拠を否定しない。
- 履歴はcontext.slot=1202003、api_version=0.23.0-alpha.0、value=[]。
  正常応答0件、next_before=null、duplicates_removed=0。
- 2ページ目はcursorがなく未実施。非空履歴、Signature/Block Height一覧、
  cursor境界・非空データの重複排除はライブ未検証。
- ページ間重複カウンタ0だが、空集合なので重複処理成功のライブ証明とはしない。
- 実通信は計4要求。5要求目を無理に使わず、追加探索を停止した。
- DNS/TLS/timeout障害なし。詳細はサーバーのアプリケーション層RPCエラー。
  診断exampleの終了0を、詳細取得成功やTX成功の根拠としない。
- 過去のexecution slotは記録値。SDK変換後の1202001はcontext由来で、TX包含高ではない。
  最新高との差分計算やFinality推測は行わない。

保持範囲外、Testnetリセット、pruning等は仮説のまま。
現在参照可能なTX/非空履歴を今回の公開記録から確認できなかったため、
新規TX作成・別ネットワーク照会・探索の追加通信は行わない。

### 変更・オフライン検証・判定

今回の変更はexamples/phase_7y_readonly.rsと本書のみ。
本番Monitor、query API、署名/送金コードは変更なし。
既存defaultモードの診断JSONは単数detail/txから配列details/tx_idsへ整理し、
複数TXも記録できるようにした。製品CLIのJSON形式は変更なし。

exampleの入力選定を関数化し、固定2件の形式、既定モード、未知引数拒否を検証する
オフラインテスト1件を追加。テストはmainを呼ばず、ネットワーク/署名なし。
専用TMPDIR・既存の許可リスト環境で
cargo test --offline --locked --example phase_7y_readonly：1成功・0失敗。
標準構成177件は今回未実行。前回52件成功を今回の成功件数へ加算しない。
既存135件を含む未実行テストは成功扱いしない。
ログ /tmp/rialo-7y-candidate-test-wsd1zg35/test.log。
cargo fmt --check、git diff --check成功。既存proc-macro-error2警告あり。

Phase 7Y総合PARTIAL。過去成功の実在TX記録と現在の検索不能を確認したが、
現在参照可能なTX詳細・非空履歴の検証目的は未達。
次は現在の公式Testnetで参照可能な公開TX IDと関連アドレス、または正式な非空fixtureを
取得することが優先。新規TX作成で検証データを補わない。
DevNet候補を使う場合は、ID/成功証拠の入手と別途DevNet通信許可が必要。
履歴/詳細のCLI統合は可能な開発候補だが、ライブ未検証範囲を明記して進める。
本作業で秘密情報読取、署名、送金、Faucet操作、TX作成、ウォレット接続、
非公式接続、commit/push、GitHub/Notion書込は行っていない。
