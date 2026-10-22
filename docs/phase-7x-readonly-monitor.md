# Phase 7X — Read-Only Transaction Monitor

## 記録時点の最終結果（2026-10-22）

- Rialo公式Testnet RPC接続とMonitor CLI動作を確認。
- ブロック高1183236、残高0 kelvin、TX状態Unknown。
- Monitor Mockテスト24件成功・0失敗。
- Finality未対応、取引履歴未実装。
- 既存135件全体の回帰テストは未完了。未実行を成功扱いしない。
- Phase 7X総合判定：PARTIAL。

以下は各段階の調査・実装・検証履歴。旧段階の未実装事項やUnconfirmed表示は
その時点の記録であり、現状は後半のCLI統合・Live Monitor CLI検証節を参照。
今回のGitHub記録作業では追加RPC・署名・送金・Faucet・Notion操作を行わない。

2026-10-22。外部ネットワークを使わず、ローカル配布ソースとMockで調査・実装・検証。
開始時main、追跡済み差分なし。既存未追跡docs/phase-7-signing-investigation.mdを保持。
Cargo.toml:7とCargo.lock:3204のrialo-cdkは0.18.1。
7U/7V/7W文書を確認。7Vの135成功は過去の結果であり今回の全件成功とはしない。

## 既存実装の所在

| 機能 | ファイル・関数・行 |
| --- | --- |
| RPC Client | src/main.rs:88 testnet_readonlyでHttpRpcClient生成、src/pre_sign.rs:218 TestnetReadRpc |
| 残高 | src/main.rs:100 testnet_readonly / get_balance、src/pre_sign.rs:225 TestnetReadRpc::balance |
| ブロック高 | src/main.rs:94 testnet_readonly / get_block_height |
| TX照会・TX状態・アドレス履歴 | 既存アプリには実装なし。SDKには後述のメソッドあり |
| エラー | src/lib.rs:9 Result（Box dyn Error）、src/main.rs:90以降map_err、src/pre_sign.rs:156 pre_sign_check_with_clockのtimeout、:222 feeの?伝播 |
| Mock RPC | src/pre_sign.rs:319 Mock、:328以降ReadRpc実装。src/pre_sign/preparation.rs:334 FeeCase（テスト用） |
| CLI | src/main.rs:23 parse、:15 Mode。--testnet-readonlyは残高/高さのみ |

src構成：lib.rs、main.rs、unsigned_tests.rs、pre_sign.rs、pre_sign/preparation.rs、
pre_sign/lifecycle.rs、pre_sign/lifecycle/persistent.rsとdiagnostics.rs、
persistent/tests/{integration,corruption,crash}.rs。トップレベルtests/なし。
今回既存CLI・署名・送金処理は変更しない。ライブラリ公開用mod宣言のみlib.rsに追加。

## SDK対応と制限

ローカル根拠の基点：
/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/
以下client.rsはrialo-cdk-0.18.1/src/rpc/client.rs。
公式配布クレートの実装・API型・コメントはローカルに存在する。
現ノードの公式RPC仕様書・実応答fixtureは今回確認できず、Web参照もしていない。

| 項目 | 分類 | 根拠・判断 |
| --- | --- | --- |
| 公開残高 | A（既存再利用もB） | client.rs:518 get_balance。既存HttpRpcClientを使用 |
| TX ID照会 | A | :586 get_transaction。TX ID引数はSignature、任意hex hashではない。詳細本文取得は今回未実装 |
| アドレス履歴 | C | :727 get_signatures_for_addressはVec<SignatureInfo>を直接復号。一方rialo-api-types-0.18.1/src/messages/get_signatures_for_address.rs:78以降はcontext/value付き応答。整合確認が必要 |
| 実行状態 | A | :619 get_signature_statuses、err/executedで判定可能 |
| ブロック高 | A（既存再利用もB） | :574 get_block_height |
| 確認数・finality | C | 状態にはerr/executed/slotのみ。:638は個別slotではなくcontext.slotを設定。高さとの差分による確認数算出は不可。finalityも未確認 |

get_signature_statusesのAPI型はVec<GetIndividualSignatureStatus>でnull要素を表現しない。
CDKのOptionは常にSomeに変換される。未発見TXの実ノード応答は未検証。
MonitorはNoneをUnknown、executed=falseをUnconfirmedとして区別するが、
実ノードでNoneが返る保証はない。SDKが復号できない応答はエラーのまま伝播する。
SDKのslotは確認数に使わず出力もしない。実行成功はfinalizedを意味しない。

## 実装

src/monitor.rs:15 MonitorRpcはbalance/height/statusesのみ公開。
:20で既存HttpRpcClientと公式RpcClientの3メソッドを委譲し、HTTP/JSON実装を重複させない。
既存署名前ReadRpcは非公開でfee/prefix用のため変更せず、送信機能を含まない専用の狭い境界を追加。
:55 snapshotは公開アドレスとbase58 Signatureを全て事前検証し、重複TXを入力順で除外。
256件上限、単一timeout、RPCエラー伝播、応答件数検証、構造化Serialize出力を実装。
残高0を正常値として扱い、errありはFailed、executed=trueはSucceeded、falseはUnconfirmed。
欠損を成功・残高0に変換しない。失敗時は部分snapshotを返さない。自動再送・pollingなし。
アドレス履歴はaddress_history_supported=false、確認数はconfirmation_depth=Noneと明示。
複数RPC読み取りは同一時点の原子的snapshotではない。現在高は個別TXの確認高ではない。
CLI接続・TX詳細本文・全履歴走査・監視結果の永続化は未実装。

## 実行前安全性監査とオフライン結果

既存135件には合成鍵生成と署名を呼ぶfixtureがある（7U/7V）。今回の署名禁止に従い
全件実行しない。7Wの安全な6件をソースで再確認した。
unsigned_testsは固定公開入力・未署名builder/復号のみ、FeeCaseはMock時刻/残高/feeのみ。
新規12件はMonitorRpcのメモリMockだけ。固定64バイトは署名IDの形式入力であり署名計算なし。
HTTPクライアントの生成も呼出もしない。timeoutはpending future、エラーは固定文字列。
秘密情報・ファイル・環境変数・送信・署名への呼出なし。依存テストは実行対象外。
7Uの依存build-script監査を参照し、今回の実行ログでは依存クレートのコンパイルなし。
専用/tmp、許可リスト環境でcargo test --offline --lockedを使用。
Cargo offlineはRPC遮断ではなく、無通信の根拠は上記Mockの呼出経路。

- monitor::tests：12成功、0失敗。
- unsigned_tests：既存5成功、0失敗。
- zero_missing_and_rpc_error_remain_distinct_in_report：既存1成功、0失敗。
- 合計18成功、0失敗。既存129件未実行、既存テスト削除・変更なし。
- 列挙：library137＋binary10＝147件、doctest0。135＋新規12を維持。
- 正常残高、0残高、TX成功/失敗/未確認/不明、不正ID、timeout、RPCエラー、重複TX、応答件数不整合を検証。
- 空履歴：履歴機能が保留のため未実施。TX指定なしと履歴非対応の表示を検証する別テストを、空履歴成功と扱わない。
- ログ：/tmp/rialo-7x-z7g4tm13/。既存proc-macro-error2将来互換性警告あり。

## 変更ファイルと残作業

新規src/monitor.rs、本書、src/lib.rsのmod宣言のみ。
Cargo.toml/Cargo.lock・既存署名/送金コード変更なし。
残作業：履歴応答形式、未発見TX、状態slot変換、finalityの公式仕様確認と
正式fixture入手。その後に履歴と空履歴試験、必要に応じたCLI接続を行う。
実RPC検証は今回対象外。SDK実装の存在だけで現ノードとの互換性を認定しない。
外部通信、秘密鍵/Seed読取、署名、TX送信、Faucet、commit/push、Notion記録なし。

## 同一依頼の再確認

既存Phase 7X実装を保持し、重複実装なしで再検証した。
main、src/lib.rsのmod追加と新規monitor/本書、既存未追跡署名調査文書の状態を確認。
専用TMPDIRと許可リスト環境で同じ安全対象だけを再実行し、
新規12＋既存6＝18成功・0失敗。既存129件は引き続き未実行。
cargo fmt --check、git diff --checkも成功。
再確認ログ：/tmp/rialo-7x-recheck-5zpoi8aw/。
再実行分を新しいテスト件数に加算しない。コード追加変更なし。

## Live Testnet Read-Only Verification（2026-10-22）

読み取りRPCのみのユーザー承認に基づき実施。7L/7Oおよび
2026-10-11-testnet-transfer-e2e.mdのendpoint・公開アドレス・送金TX IDを再利用。
新しい接続先・TXを作成していない。

- 接続先： https://testnet.rialo.io/ （src/lib.rsのTESTNET_RPC）
- 公開アドレス：7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa
- 既知TX：5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs
- 実行：./target/debug/examples/phase_7x_readonly --live-readonly
- 開始：2026-10-22 11:32:22 UTC / 20:32:22 JST
- 完了：2026-10-22 11:32:24 UTC / 20:32:24 JST、exit 0

| 検証 | 判定 | 取得値・時刻（JST） |
| --- | --- | --- |
| RPC接続 | PASS | 最初のconfig RPC成功、20:32:23 |
| Network Config Prefix | PASS | 13223746993924564309、20:32:23 |
| 最新ブロック高 | PASS | 1178070、20:32:24 |
| 公開残高 | PASS | 0 kelvin = 0 RLO、20:32:24 |
| 既知TX状態 | PARTIAL | SDK照会成功、Monitor出力Unconfirmed、20:32:24 |
| 履歴・確認数・finality | NOT TESTED | 今回対象外・未対応 |

TX状態RPCは正常に復号でき、Monitorの判定経路ではexecuted=falseかつerrなしに該当。
生JSONは保存していないため、記録はSDK変換後の結果である。
過去の2026-10-11のexecuted=trueという観測を否定しない。
現在ノードの未発見TXの扱い、検索対象期間、Testnetリセット有無は不明。
Unconfirmedは現在のSDK応答をMonitorが分類した値であり、現在のmempool滞留の証明ではない。
過去7L/7Oとconfig prefix・高さが異なるが、その原因は未調査。
config prefixは独立したchain identity証明ではなく、各値は別RPC時点の観測。

### 通信・エラー・安全性

最初のsandbox内実行は20:31:36 JSTにDNS解決失敗
（Temporary failure in name resolution）で停止。RPC応答なし。
サンドボックス制限外の実行承認後に上記1回を実施。
成功した実行ではconfig→高さ→残高→TX状態の4要求のみ。
接続確認はconfig成功を兼用。高さをキャッシュしてMonitor内で再照会せず、
残高・状態は既存MonitorRpcのHttpRpcClient実装へ委譲しsnapshotを実際に実行した。
自動リトライなし、全体30秒・snapshot20秒timeout。認証環境を継承しないenv -iで起動。
実RPC以外の外部通信なし。秘密鍵・seed・ウォレット設定読取、署名、送信、Faucetなし。
Git commit/push・GitHub書込・Notion書込なし。明示禁止を維持した。

### 実装変更・検証と次の作業

手動検証用examples/phase_7x_readonly.rsを追加。--live-readonly明示時のみ実行可能。
既存Monitor/署名/送金コードは変更なし。通常のcargo testからネットワーク処理は起動しない。
cargo build --offline --locked --example phase_7x_readonly成功。
cargo fmt --exampleは非対応オプションで失敗したため、対象をrustfmtで整形。
最終cargo fmt --check / git diff --check成功。
既存proc-macro-error2将来互換性警告あり。重複するオフラインテストは再実行していない。

次はローカルSDK/API定義で未発見TXの状態表現と個別slot喪失を調査し、
Unconfirmedと未発見を誤解させない出力へ改善すること。
公式のノード仕様/fixtureが不足する部分は未確認として維持する。
その後に履歴APIの応答形式の整合確認とCLI統合を進める。
今回の成功は現在の残高・高さ・config照会とMonitor実行経路の検証に限り、
TX成功/失敗/未発見の全ケース、履歴、finalityのライブ検証完了とはしない。

## CLI Integration（2026-10-22、オフラインのみ）

### 使用方法

```sh
cargo build --offline --locked --bin rialo-first
./target/debug/rialo-first --testnet-monitor --address 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa
./target/debug/rialo-first --testnet-monitor --address 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa --tx-hash 5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs --timeout-secs 10
```

上記は使用例であり、今回実RPCへ実行していない。
--tx-hashは複数指定可。同一IDはMonitorで重複排除し、異なるIDは最大256件。
常にJSONをstdoutへ出力。--jsonは不要・未対応。
--rpcは省略可能で、指定時は既存TESTNET_RPCのhttps://testnet.rialo.io/との完全一致のみ許可。
別ネットワーク、認証情報、query等を含むURLは拒否する。
--timeout-secsは1〜30秒、既定30秒。configを含む全体timeout。
未知・重複引数、欠落値、不正公開アドレス/署名IDはHTTPクライアント生成前に拒否。
TX Hashという引数名でも実際の入力形式は公式Signatureのbase58表現。

### 出力と状態の意味

address、balance_kelvin、block_height、network_config_prefix、rpc、transactions、
address_history_supported=falseをJSONで返す。確認数・Finalityは出力しない。
ConfigRpcはMonitorRpcを拡張するconfig専用の注入境界で、公式get_config_hash_prefixへ委譲。
既存snapshotとHttpRpcClientの実装を再利用し、独自HTTP/JSON-RPC transportを増やしていない。

- Confirmed：公式状態型のexecuted=true、errなし。既存SucceededをCLI表示で変換。Finality保証ではない。
- Failed：公式状態型にerrあり。
- Unknown：None、またはexecuted=falseでerrなし。SDKが未発見と処理待ちを区別できないため。
- Unconfirmed：型は保持するが、現在SDKで明確な未確認応答を識別する根拠がないため出力しない。

この変更により、以前のライブ観測Unconfirmedは旧判定の記録として保持する。
同じ状態応答を現実装へ渡すとUnknownになる。今回再照会していない。
Monitorのconfirmation_depthフィールドは互換性のため保持しSerialize対象から除外した。

成功時は終了コード0、RPC/応答エラーやtimeout時はJSONのerrorとstderrへ固定の理由を出し終了1。
SDK由来の任意のエラー本文・URLや入力値はログに出さない。
入力検証エラーは既存CLIと同じstderr/終了1で、JSON出力前に停止する。

### 変更と安全性

今回の追加変更：src/main.rs（専用Mode/分岐/help/テスト）、src/monitor.rs
（共通入力検証関数、曖昧な状態をUnknown、確認数を非表示）、
src/monitor/cli.rs（引数・config委譲・JSON・終了結果・Mockテスト）、本文書。
既存送金準備と署名前ライフサイクルの処理は変更なし。
既存examples/phase_7x_readonly.rsは変更せず、今回起動していない。
新規依存・Cargo.lock変更なし。前段のsrc/lib.rsのmod宣言をそのまま使用。

実行前監査：新規CLIテストはメモリMockと固定公開入力のみ。
成功、0残高、高さ/config、TX成功/失敗/欠損/曖昧状態、timeout、エラー秘匿、
入力不正、URL制約、JSON、終了コードを検証。
本番run（HttpRpcClient生成）はテストから呼ばずexecuteへMockを渡す。
追加binaryテストはparseのみ。既存binary10件はparse/remaining/未署名messageのみで、
アプリmain/run、署名、送信を呼ばないことを再確認した。
既存unsigned5件とFeeCase1件は7U/7Wの監査を再利用。
署名fixtureを使う残りの既存library119件は実行しない。

### 結果

専用TMPDIRと許可リスト環境でcargo test --offline --lockedを対象指定して実行。
ログ：/tmp/rialo-7x-cli-n9pbg2f0/。

- monitor（既存12＋今回新規12）：24成功。
- binary（既存10＋今回新規1）：11成功。
- unsigned_tests：既存5成功。
- FeeCase：既存1成功。
- Rustテスト合計41成功、0失敗、119未実行。全構成160件（library149＋binary11）。
- 今回新規13件。元の135件は維持し、そのうち16件実行、119件未実行。
- 前段Monitorの12件のうち旧tx_unconfirmedは曖昧状態Unknownを期待する試験へ更新。
- 実CLIのhelp終了0、不正アドレス/不正TX/認証付きURLの各終了1を別途4ケース確認。
  全て通信前終了、認証文字列非表示。Rustテスト件数には加算しない。
- cargo build --offline --locked --bin rialo-first、cargo fmt --check、git diff --check成功。
- proc-macro-error2の既存将来互換性警告あり。

CLI統合と対象オフライン検証は完了。今回実RPC、秘密情報読取、署名、送信、Faucet、
commit/push、Notion記録なし。ライブCLI検証、履歴API整合、未発見状態の公式仕様、
確認数/Finalityは未実装・未検証として残す。Mock成功をライブCLI成功とは扱わない。

## Live Monitor CLI Verification（2026-10-22）

### 環境・実行

作業ディレクトリ /home/pc/rialo-first。kernelは6.18.33.2-microsoft-standard-WSL2。
既存CLIと既存公式URL TESTNET_RPC=https://testnet.rialo.io/ を使用。
既存記録2026-10-11-testnet-transfer-e2e.mdの公開アドレスと送金TX IDを再利用。
照会順を指定どおりにするため、src/monitor.rsの高さ・残高の2呼出の順番のみ変更。
config→高さ→残高→TX状態の計4要求、自動リトライなし。
cargo build --offline --locked --bin rialo-first成功後、下記CLIを1回だけ実行した。

```sh
getent ahostsv4 testnet.rialo.io
./target/debug/rialo-first --testnet-monitor --address 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa --tx-hash 5h5JFQgEdyS6fJzVCugGxdbfcg7Wj6auUGmPuLPMV76u1vFENhAhWeuUK2UnrFGvW1nKTb9EXit7QsJqL8RushTs --timeout-secs 20
```

DNS確認は10秒、CLIは外側25秒・内部20秒で制限。PATH/LANGのみの環境で起動。
実行開始2026-10-22 11:45:01.261402 UTC（20:45:01.261402 JST）、
終了11:45:03.230108 UTC（20:45:03.230108 JST）。取得時刻はこの実行区間。
個々のRPC応答時刻はCLIが出力しないため、個別時刻を推測して付与しない。
結果・実行コマンド・DNS出力・時刻は
/tmp/rialo-7x-live-cli-c548nhf_/result.jsonに保存。
生HTTP/JSON-RPC応答ではなくCLIのSDK変換後JSON出力である。

### 接続の切り分け

sandbox内getentは終了2・結果なし。制限外実行の承認後にgetent成功、
3.138.227.249と13.184.85.226を返した。得られたIPへURLを変更せず公式URLを使用。
同じWSL2環境の制限外でDNSとHTTPS/RPCが成功したため、sandbox制限と整合する。
sandbox側getentは低レベルエラーを出さないので遮断の内部機構までは断定しない。
WSL2全体の継続的DNS障害は今回再現していない。
HTTPS/TLSは証明書検証を有効にした既存HttpRpcClientでRPC成功したことで確認。
別のHTTP診断通信は行っていない。HTTPS障害・RPCサーバーエラーは成功実行で観測なし。
DNS/ネットワーク設定変更なし、非公式URLへの切替なし。

### 取得結果と判定

| 項目 | 判定 | 結果 |
| --- | --- | --- |
| 公式RPC接続/DNS/HTTPS | PASS | configを含む読み取り成功 |
| Network Config Prefix | PASS | 13223746993924564309 |
| 最新ブロック高 | PASS | 1183236（正の整数） |
| 公開残高 | PASS | 0 kelvin = 0 RLO（1 RLO=10^9 kelvin） |
| 既知TX状態の照会処理 | PASS | SDK復号とCLI JSON出力成功 |
| 既知TXの実行結果確定 | PARTIAL | Unknown。成功・失敗・未発見の区別を確定できない |
| 履歴・確認数・Finality | NOT TESTED | 未対応、確認数/Finalityの出力なし |

CLI終了0、stderrなし。JSONの型・値域・接続先・Unknown・確認数非表示をローカル検証。
UnknownはSDKが区別できない状態であり、TX成功とは扱わない。
未発見の可能性を含むが、明示的な未発見応答を識別できないため「未発見確定」とも扱わない。
今回のUnknownは、正常復号した1件の状態についてerrなし/ executed=falseとなる既存判定に基づく。
SDKはOptionを常にSomeへ変換するため、今回のUnknownをnull応答の観測と説明しない。
過去の成功記録を取り消さない。Testnetの履歴保持/リセット/未発見表現は引き続き未確認。
RPCエラー時にはsnapshotが失敗する設計で、エラーを0残高へ変換していない。
各値は別RPCで取得し、原子的snapshotではない。configは独立したchain IDの証明ではない。

判定：残高・高さ取得とMonitor CLI動作はPASS。
TX結果の確定まで含めた総合判定はPARTIAL。Unknownを成功に加算しない。

### 変更・追加検証・次の作業

今回変更：src/monitor.rsの高さ/残高の照会順、本書の追記のみ。
順番変更の回帰確認として安全なmonitor::のMockテストだけ再実行：24成功・0失敗。
他の136件は今回未実行（全構成160件）。署名fixtureは起動せず。
ログ /tmp/rialo-7x-order-check-mqzjqzzt/test.log。
cargo fmt --check / git diff --check成功。既存proc-macro-error2警告あり。
次は公式仕様または正式fixtureから未発見TXの表現と履歴保持範囲を確認し、
Unknownの理由表示を改善する。履歴API形式の整合、CLIの観測時刻表示も残課題。
本作業での秘密鍵/Seed/ウォレット情報読取、署名、送金、Faucet、ウォレット接続、
TX送信、依存追加、commit/push、GitHub/Notion書込はなし。
