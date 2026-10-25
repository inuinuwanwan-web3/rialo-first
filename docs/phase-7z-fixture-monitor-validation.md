# Phase 7Z — Transaction History Fixture & Monitor Validation

## 1. 目的・引き継ぎ

Phase 7Yの詳細取得・非空履歴・ページネーションの未検証範囲を、実RPCなしで検証する。
7YのTestnetでは過去TX詳細がTransaction not foundとなった。本作業では再照会しない。
開始時HEADは040bd95。既存の未追跡docs/phase-7-signing-investigation.mdは変更しない。
Cargo.toml / Cargo.lockを確認し、rialo-cdk =0.18.1を維持。依存追加なし。

## 2. SDK型の確認

導入済みレジストリソースだけを参照。以下のパスは
`/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` 相対。

| 型 | ソース・行 | 確認事項 |
|---|---|---|
| GetTransactionResponse | rialo-api-types-0.18.1/src/messages/get_transaction.rs:12 | context, block_height, transaction, meta |
| Transaction | 同上:34 | signatures, message, validFrom。executedフィールドなし |
| TransactionStatusMetadata | 同上:85 | errはOption<Value> |
| SignatureStatus | rialo-cdk-0.18.1/src/generated/types.rs:141 | slot, executed, err: Option<String> |
| GetIndividualSignatureStatus | rialo-api-types-0.18.1/src/messages/get_signature_statuses.rs:60 | wire側errはSDK TransactionError |
| SignatureInfo | rialo-api-types-0.18.1/src/messages/get_signatures_for_address.rs:88 | signature, blockHeight, err: Option<TransactionError> |
| GetSignaturesForAddressResponse | 同上:71 | context/value形式 |
| GetSignaturesForAddressConfig | 同上:32 | before/untilはOption<String>、独立Cursor型なし |
| TransactionError | rialo-shared-types-0.18.1/src/lib.rs:266 | 履歴用enum、AccountNotFoundを使用 |

CDKのrpc/types.rs:299,400等の再export経由で既存Monitorと同じ型を使用。
詳細のerrはJSON値、公開SignatureStatusのerrは文字列であり、履歴の型付きenumと混同しない。
fixtureのhistory_failureは実際にSignatureInfoへ復号し、errを保持したJSON出力も確認。
JSON nullや不足レスポンスは正常なGetTransactionResponseを捏造せず、負の入力として扱う。
Optionフィールドの省略とnullはSDKのserde仕様上区別できない場合がある。

## 3. fixture一覧

`tests/fixtures/phase7z/synthetic.json` 1ファイル、A〜Hの8カテゴリ。
詳細は同ディレクトリREADME.md。すべてsyntheticと明記。
3つのSignatureは固定反復バイトのbase58表現。実在TXや暗号学的署名を主張しない。

| カテゴリ | 内容 |
|---|---|
| A | executed=true/err=null、成功詳細、block height |
| B | executed=true/errあり、失敗詳細、型付き履歴エラー |
| C | executed=false/err=null |
| D | 詳細null、状態なしはUnknown |
| E | Signatureとheightが異なる3件、height非ソート |
| F | 1ページ目、短い終端ページ、空の終端ページ、Signature cursor |
| G | 第2ページの境界重複と新しいTX |
| H | RPCエラー、pending futureによるタイムアウト、不正ID/cursor、不完全/空応答 |

RPCエラー/タイムアウトはSDKレスポンス型ではなくMockの動作として注入する。
未実行TXの詳細にSDKにないexecutedは追加しない。

## 4. 既存機能と追加実装

既存: MonitorRpc/QueryRpc/ConfigRpcによる注入、transaction_detail、observe_status、
status_reason、history_page、ページ内重複排除、CLI JSON/秘匿エラー/timeout。
これらをそのまま使用し、テスト専用Monitorは作成しない。

新規: query::history_pagesとHistoryBatch。既存history_pageを再利用し、
1〜100ページの明示予算、全体timeout、ページ間重複排除、矛盾する重複の拒否、
cursor循環の拒否、次回候補cursor保持、取得ページ数/除去数のJSON化を追加。
最初に観測した順を保持し、block heightでソートしない。
既存CLI・状態enum・署名/送金実装は変更なし。

## 5. 状態・詳細テスト

新規11テストで複数ケースを検証。
executed=true/err=nullはSucceeded / ExecutedWithoutError、errありは
Failed / RpcReportedError。false/nullはUnknown / UnexecutedOrNotFound。
状態なしはUnknown / NoStatusReturned。不完全statusは復号エラー。
詳細取得だけでは状態を判定せず、詳細null/欠損はエラー、状態なしはUnknownのまま。
RPC/status取得失敗・不正なstatus件数はエラーとして伝播し、Successへ変換しない。
Pending/Not Foundの独自判定は追加しない。CLI成功表記Confirmedは既存互換のため維持。
詳細Signature/height/errとCLI execution/status_reason JSONの整合を確認。
既存CLIテストでtimeout時の終了コード・サニタイズ済みエラー表示も確認。

## 6. 非空履歴・ページネーション・重複

3件のID/height/件数/受信順を検証。順序の公式保証は仮定しない。
page1 → 重複を含むpage2 → 空ページで終端。3件の有効TXを維持し、重複1件のみ除去。
cursorは生のページ末尾を使用し、要求への引き渡しをMockで確認。
短いページ/空ページは次候補なし。満杯ページは次ページ存在の保証ではない。
ページ予算到達時は次候補を返し、履歴全体の完全取得と主張しない。
矛盾する重複・循環・無効cursor・無効page予算はエラー。

## 7. 安全確認と回帰結果

実行前に全ターゲットを`--list`で列挙（テスト本体は実行しない）。既存178件。
Monitor配下はMock/固定応答で外部通信なし。mainのCLI解析/算術9件と
7Y exampleの候補選択1件も通信・署名・TX作成なしと確認。
libの他125件は署名/PreparedTransfer生成等を含む系統のため保守的に除外。
mainのcanonical_unsigned_transferとpreparation_cli_testsの2件もTX作成を伴うため除外。
examplesのmainは呼び出さず、依存SDK自身のテストも実行しない。

| 項目 | 件数 |
|---|---:|
| 既存総数 | 178 |
| 新規 | 11 |
| 合計 | 189 |
| 既存実行 | 51（Monitor 41、CLI 9、example 1） |
| 新規実行 | 11 |
| 実行合計・成功 | 62 |
| 失敗 | 0 |
| 未実行 | 127 |

再実行による重複加算なし。未実行は成功扱いしない。

実行コマンドと結果:

- `cargo test --offline --locked --test phase_7z_monitor`: 11成功。
- `cargo test --offline --locked --lib monitor::`: 41成功、125除外。
- `cargo test --offline --locked --bin rialo-first -- --skip canonical_unsigned_transfer --skip preparation_cli_tests`: 9成功、2除外。
- `cargo test --offline --locked --example phase_7y_readonly`: 1成功。
- `cargo fmt --check`: 成功。
- `git diff --check`: 成功。
- `cargo build --offline --locked`: 成功。

Cargoの既存依存proc-macro-error2の将来互換警告あり、build/testの失敗なし。
集計補助スクリプトの括弧誤りは修正して再集計済み。テスト失敗ではない。
外部通信、秘密情報読み取り、署名、TX作成/送信、Faucet、wallet接続、commit/push、Notion操作なし。

## 8. 未検証事項・最終判定

**総合PARTIAL**。今回追加機能と安全確認済み回帰62件のオフライン検証は成功。
ユーザー指定のPARTIAL条件「テストに未実行項目が残る」に従い、127件を残してPASSとしない。
未実行は安全制約による除外であり、失敗や既存機能破損を観測したものではない。

実装済み: 上記bounded複数ページ集約。オフライン検証済み: synthetic fixtureによる
詳細/状態/非空履歴/境界重複/JSON/エラー/timeout。実Testnet未検証: 非空履歴、
詳細取得成功、実サーバの順序・cursor挙動、履歴保持期間、finality。
オフライン成功は実Testnet成功を意味しない。

次の候補: 書き込み系から安全な純粋テストを分離する回帰対象整理、および別途明示許可された
段階での実Testnet非空履歴・詳細照会の検証。


## Safe Regression Test Audit（2026-10-25）

### 今回の最終判定

**PASS — 安全なオフライン回帰テストの監査・実行完了。**
上の初回PARTIALは当時の基準と結果として残す。今回の基準では、C/Dを実行しないことは
PASSを妨げず、A/B全件成功・分類完了・build/整形/差分チェック成功を満たした。
実Testnet成功、禁止対象のテスト成功、189件全件の回帰成功を意味しない。

### 件数の照合と定義

`cargo test --offline --locked --all-targets -- --list`で本体を実行せず一覧取得。
集計単位は **Cargoターゲット種別 + ターゲット名 + 完全修飾テスト名**。
macro展開された個々のテストもlibtest登録名で1件とし、macro定義自体は数えない。
assertion数・fixture数・内部のケースループ・子プロセスの同名実行・再実行は加算しない。
依存SDKのテストと通常のexample mainも対象外。

| ターゲット | 登録数 | 今回成功 | 未実行 |
|---|---:|---:|---:|
| lib rialo-first | 166 | 50 | 116 |
| bin rialo-first | 11 | 9 | 2 |
| integration phase_7z_monitor | 11 | 11 | 0 |
| example phase_7x_readonly | 0 | 0 | 0 |
| example phase_7y_readonly | 1 | 1 | 0 |
| 合計 | 189 | 71 | 118 |

既存178件 + Phase 7Z新規11件 = 189件で前回値と一致。
前回実行対象はMonitor 41 + bin 9 + example 1 + integration 11 = 62件。
差集合127件を個別監査。今回新規テスト追加なし、追加実行9件。
今回71成功・0失敗・118未実行。累積のユニーク成功件数も71件（62+71とは数えない）。

| 分類 | 監査対象127件 | 全189件の分類 | 今回実行 |
|---|---:|---:|---:|
| A: 完全オフラインで安全 | 8 | 18 | 18 |
| B: 既存Mock/fixtureで外部依存なし | 1 | 53 | 53 |
| C: 実RPC/外部通信が必要 | 0 | 0 | 0 |
| D: TX作成・実署名・秘密鍵操作等 | 118 | 118 | 0 |
| 未分類 | 0 | 0 | 0 |

### 呼び出し先・初期化・環境・副作用の監査根拠

- `src/lib.rs:60`のnew_with_amountは金額/network/address/weak-key確認後、同:92付近で
  build_transactionを呼ぶ。unsignedやMockという名前でもこの境界を越えるテストはD。
  `src/unsigned_tests.rs`のinvalid_or_missing_address_and_network_rejectedだけは全ケースが
  上流で拒否され、TX生成なし。
- `src/pre_sign.rs:353`のfixtureは公開basepointからPreparedTransferを作る。
  private keyやRPCは不要でもTX生成があるので、そのfixture/timedを使うテストはD。
  invalid_fee_data_is_rejected_by_cdk_response_typeはJSON復号のみでfixture非使用なのでA。
- `src/pre_sign/preparation.rs:12`のparseは正常入力ではPreparedTransferを作る。
  invalid_inputsの全ケースは金額・引数・address等の拒否経路で止まりA。
  その他の正常入力を使うMockテストはD。
- `src/pre_sign/lifecycle.rs:325,357,367`のcreated/awaiting/responseはSigningKey操作、
  TX生成、key.signを含む。テスト本体にsignがなくても共通fixture経由をDに分類。
- `src/pre_sign/lifecycle/persistent.rs:425,440`のchecked/readyも同様。
  `tests/corruption.rs:3`の共通fixtureもcheckedを呼ぶので、破損検査という名前だけでAにしない。
- 保存系Aはchecked/readyを呼ばない空ストアのみ。
  initialize（persistent.rs:120）はcreate/create_newで既存を上書きしない。
  loader（persistent/diagnostics.rs:88付近）は指定journalの権限・排他・JSON等を検査し、
  署名やネットワークclientを生成しない。
- Temp（persistent.rs:389）にはDropでremove_dir_allがある。
  実行はPython tempfileが新規作成した専用`/tmp/phase7z-safe-regression-*`をTMPDIRとして
  子プロセスだけに渡した。既存TempのPID/counter名もその配下となり、既存ファイルに衝突しない。
  終了後に専用rootを削除。ホーム、設定、既存journalは対象にしない。
- cross_process_lock_refuses_second_userはcurrent_exeの同名テストだけを起動し、専用tempの
  空journalに対するロック拒否を確認。子内部実行は別件に加算しない。
- `tests/integration.rs:99,153`のrun/scenarioを分岐まで確認。
  connection_failureのみMock App.connectでSigningKey操作前にreturnし、expected=Noneなので
  scenario後半のcheckedも呼ばない。従ってB。他のcaseはTX生成や実署名を含む経路がありD。
- crash.rs:90のchild_processは環境がないとreturnするが、それだけを「検証済み」と数えない。
  環境指定時のready/署名経路まで含めてDとし、crashテスト全体も除外。
- 子テストの経路を変える`RIALO_7H_TEST_*`と`RIALO_PERSISTENCE_LOCK_TEST`は
  実行子プロセス環境から除去。cross-processテスト自身が必要な値だけを子へ渡す。
  永続環境変数やネットワーク設定は変更していない。
- 既存Monitor/CLI/Phase7Z fixtureテストはメモリMock/固定レスポンスを使用。
  ConfigRpc/MonitorRpc/QueryRpcから実HttpRpcClientを呼ばない。対象exampleは候補選択関数のみ。

Dには本来オフラインでも実行可能なテストも含む。本作業では「TX作成禁止」「秘密鍵を使った
実署名禁止」を優先。TX作成なしのMockへ置き換えると本来の検証対象を失うため変更しない。
mainのcanonical_unsigned_transferも転送message構築を伴うため保守的にDを維持。

### 監査対象127件の個別分類

行番号はテスト関数またはmacro呼び出しの位置。macro定義の展開先を重複計上しない。
共通fixtureと副作用の根拠は上記と各行の理由を併読。

| ターゲット / テスト完全名 | ソース:行 | 分類 | 根拠 |
|---|---|---|---|
| lib / `pre_sign::lifecycle::persistent::tests::abandoned_awaiting_request_cannot_resume` | `src/pre_sign/lifecycle/persistent.rs:560` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::checksum_corruption_refused` | `src/pre_sign/lifecycle/persistent.rs:666` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::concurrent_handles_refused` | `src/pre_sign/lifecycle/persistent.rs:626` | A | 空ストアの初期化と排他検証のみ。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::absent_directory_not_auto_initialized` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:147` | A | Tempの未作成パスをopenしMissing確認。fixtureなし。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::conflicting_temp_file_stops` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:129` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::directory_permissions_rejected` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:108` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::duplicate_registration_with_valid_checksum` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:82` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::file_permissions_rejected` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:102` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::invalid_transition_with_valid_checksum` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:75` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::malformed_json_redacted` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:42` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::matching_temp_file_stops` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:122` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::missing_fields_rejected` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:59` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::missing_file_not_initialized` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:114` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::normal_state_preserved_and_id_blocked` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:139` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::truncated_json_preserved` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:52` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::unknown_field_never_disclosed` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:65` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::unknown_protocol_rejected` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:95` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::unknown_schema_rejected` | `src/pre_sign/lifecycle/persistent/tests/corruption.rs:89` | D | 共通fixture→checkedでSigningKey操作とTX生成。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::child_process` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:90` | D | 環境変数でready/署名/TX生成する子プロセス入口。通常の早期returnを検証成功と数えない。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_after_response_sync` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:214` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_after_signed_sync` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:221` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_before_response_sync` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:208` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_before_response_write` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:196` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_during_validation` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:227` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_partial_response_json` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:202` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::kill_partial_signed_json` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:219` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::missing_file_rejected_without_recreation` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:244` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::permissions_rejected_without_modification` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:255` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::crash::save_failure_then_fresh_process_rejects_reuse` | `src/pre_sign/lifecycle/persistent/tests/crash.rs:234` | D | baseline→checked、子process→ready。TX生成/実署名を伴う。 |
| lib / `pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user` | `src/pre_sign/lifecycle/persistent.rs:707` | A | current_exeの同一テスト子だけを起動。専用tempの空journal排他のみ。 |
| lib / `pre_sign::lifecycle::persistent::tests::duplicate_and_different_message_id_refused` | `src/pre_sign/lifecycle/persistent.rs:532` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::expired_survives_restart` | `src/pre_sign/lifecycle/persistent.rs:510` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::final_save_failure_withholds_wire_and_restart_blocks_retry` | `src/pre_sign/lifecycle/persistent.rs:684` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::connection_failure` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:196` | B | App.connectのMockエラーでrunはSigningKey生成前に終了。scenarioのexpected=Noneでchecked分岐にも入らない。空journalだけを作る。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::construction_failure` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:200` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::dummy_signer_failure` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:202` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::execution_failure` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:226` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::fee_exceeds_limit` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:199` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::insufficient_balance_including_fee` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:197` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::missing_fee` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:198` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::mock_send_failure_no_retry` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:214` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::offline_pipeline_success` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:195` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::save_failure_blocks_send` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:208` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::timeout_no_retry` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:225` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::unknown_result_no_retry` | `src/pre_sign/lifecycle/persistent/tests/integration.rs:220` | D | scenario→run/再登録checkedでSigningKey操作またはTX生成。sendがMockでも実署名を含む経路あり。 |
| lib / `pre_sign::lifecycle::persistent::tests::missing_and_corrupt_files_refused` | `src/pre_sign/lifecycle/persistent.rs:634` | A | 専用temp内の空/不正journalのみ。checked/readyなし。 |
| lib / `pre_sign::lifecycle::persistent::tests::normal_save_reload_and_signed_replay` | `src/pre_sign/lifecycle/persistent.rs:462` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::permissions_restricted_and_insecure_file_refused` | `src/pre_sign/lifecycle/persistent.rs:650` | A | 専用temp内の空journal権限のみ。 |
| lib / `pre_sign::lifecycle::persistent::tests::registration_io_failure_stops_and_poisons` | `src/pre_sign/lifecycle/persistent.rs:576` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::rejected_survives_restart` | `src/pre_sign/lifecycle/persistent.rs:487` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::request_not_exposed_on_save_failure` | `src/pre_sign/lifecycle/persistent.rs:595` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::persistent::tests::response_not_processed_on_claim_save_failure` | `src/pre_sign/lifecycle/persistent.rs:613` | D | checked/readyでSigningKey操作・TX生成、readyはkey.signも実行。 |
| lib / `pre_sign::lifecycle::tests::cancelled_validation_is_terminal` | `src/pre_sign/lifecycle.rs:570` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::clock_failure_rejects_response` | `src/pre_sign/lifecycle.rs:538` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::expired_checks_block_request_creation` | `src/pre_sign/lifecycle.rs:530` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::expiry_during_assembly_never_returns_wire` | `src/pre_sign/lifecycle.rs:550` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::fee_excess_blocks_request_creation` | `src/pre_sign/lifecycle.rs:515` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::ids_are_distinct_within_process` | `src/pre_sign/lifecycle.rs:584` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::network_mismatch_blocks_request_creation` | `src/pre_sign/lifecycle.rs:507` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_amount_tampering` | `src/pre_sign/lifecycle.rs:470` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_double_response_and_signed_request_reuse` | `src/pre_sign/lifecycle.rs:490` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_expired_response_and_reuse` | `src/pre_sign/lifecycle.rs:480` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_fee_display_tampering` | `src/pre_sign/lifecycle.rs:476` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_hash_mismatch` | `src/pre_sign/lifecycle.rs:447` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_invalid_signature` | `src/pre_sign/lifecycle.rs:434` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_message_tampering` | `src/pre_sign/lifecycle.rs:464` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_missing_signature` | `src/pre_sign/lifecycle.rs:450` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_network_tampering` | `src/pre_sign/lifecycle.rs:473` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_protocol_version_mismatch` | `src/pre_sign/lifecycle.rs:444` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_public_key_mismatch` | `src/pre_sign/lifecycle.rs:437` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_recipient_tampering` | `src/pre_sign/lifecycle.rs:467` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_rejected_session_revalidation` | `src/pre_sign/lifecycle.rs:498` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rejects_request_id_mismatch` | `src/pre_sign/lifecycle.rs:440` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::rpc_failure_blocks_request_creation` | `src/pre_sign/lifecycle.rs:523` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::valid_request_fields_come_from_message_and_checks` | `src/pre_sign/lifecycle.rs:377` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::lifecycle::tests::valid_response_builds_local_wire_and_signature_verifies` | `src/pre_sign/lifecycle.rs:400` | D | created/awaitingでSigningKey操作とTX生成。responseはkey.signを実行。 |
| lib / `pre_sign::preparation::tests::config_mismatch_blocks` | `src/pre_sign/preparation.rs:323` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::fee_limit_exceeded` | `src/pre_sign/preparation.rs:300` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::insufficient_balance_for_estimate` | `src/pre_sign/preparation.rs:293` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::insufficient_balance_for_limit` | `src/pre_sign/preparation.rs:284` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::invalid_inputs` | `src/pre_sign/preparation.rs:262` | A | 全ケースが引数・金額・address検証でTX生成前に拒否される。 |
| lib / `pre_sign::preparation::tests::missing_fee_blocks` | `src/pre_sign/preparation.rs:307` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::rpc_failure_blocks` | `src/pre_sign/preparation.rs:316` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::success_and_never_signed_or_broadcast` | `src/pre_sign/preparation.rs:250` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::preparation::tests::zero_missing_and_rpc_error_remain_distinct_in_report` | `src/pre_sign/preparation.rs:331` | D | Input::parse/prepare_usingでPreparedTransferを生成。 |
| lib / `pre_sign::tests::backward_and_divergent_clocks_stop` | `src/pre_sign.rs:545` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::clock_read_failure_stops` | `src/pre_sign.rs:555` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::config_change_during_queries_stops` | `src/pre_sign.rs:467` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::fee_over_limit_stops` | `src/pre_sign.rs:420` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::initial_check_expiring_during_rpc_is_rejected` | `src/pre_sign.rs:696` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::insufficient_balance_stops` | `src/pre_sign.rs:425` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::invalid_clock_and_ttl_stop_before_rpc` | `src/pre_sign.rs:522` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::invalid_fee_data_is_rejected_by_cdk_response_type` | `src/pre_sign.rs:438` | A | JSON復号のみ。fixture()を呼ばない。 |
| lib / `pre_sign::tests::matching_prefix_and_exact_message_succeed` | `src/pre_sign.rs:379` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::missing_fee_stops` | `src/pre_sign.rs:414` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::modified_message_stops_before_rpc` | `src/pre_sign.rs:431` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::overflowing_total_stops` | `src/pre_sign.rs:460` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::prefix_mismatch_stops` | `src/pre_sign.rs:400` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::result_expires_exactly_at_deadline` | `src/pre_sign.rs:513` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::result_valid_before_deadline` | `src/pre_sign.rs:504` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_cannot_renew_old_result_that_expires_during_rpc` | `src/pre_sign.rs:705` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_refreshes_increased_fee_within_original_limit` | `src/pre_sign.rs:618` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_amount_change` | `src/pre_sign.rs:603` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_corrupt_or_missing_report_data` | `src/pre_sign.rs:658` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_fee_limit_excess` | `src/pre_sign.rs:632` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_insufficient_balance` | `src/pre_sign.rs:639` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_message_change` | `src/pre_sign.rs:595` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_network_change` | `src/pre_sign.rs:611` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_public_key_change` | `src/pre_sign.rs:607` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_recipient_change` | `src/pre_sign.rs:599` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_rejects_rpc_errors_and_missing_fee` | `src/pre_sign.rs:646` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::revalidation_success_with_unchanged_state` | `src/pre_sign.rs:715` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `pre_sign::tests::rpc_errors_stop` | `src/pre_sign.rs:406` | D | fixture/timedでPreparedTransfer::new→build_transaction。RPCがMockでもTX生成を伴う。 |
| lib / `tests::rejects_invalid_addresses_and_same_recipient` | `src/lib.rs:233` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::rejects_invalid_and_malformed_signatures` | `src/lib.rs:176` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::rejects_missing_and_duplicate_signatures` | `src/lib.rs:213` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::rejects_non_testnet_endpoints` | `src/lib.rs:261` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::rejects_signatures_for_different_message_or_encoding` | `src/lib.rs:275` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::rejects_wrong_public_key_and_wrong_signing_key` | `src/lib.rs:190` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `tests::valid_signature_round_trips_exact_message` | `src/lib.rs:151` | D | fixtureでSigningKey生成、prepareでTX生成。複数ケースでkey.signも実行。 |
| lib / `unsigned_tests::deterministic_and_sensitive_to_all_transaction_inputs` | `src/unsigned_tests.rs:48` | D | prepared→new_with_amount→build_transaction。 |
| lib / `unsigned_tests::fee_payload_is_message_not_unsigned_envelope` | `src/unsigned_tests.rs:73` | D | prepared→new_with_amount→build_transaction。 |
| lib / `unsigned_tests::invalid_or_missing_address_and_network_rejected` | `src/unsigned_tests.rs:99` | A | 全入力がnew_with_amountの金額/network/address検証でbuild_transaction前に拒否される。 |
| lib / `unsigned_tests::malformed_unsigned_envelopes_rejected` | `src/unsigned_tests.rs:89` | D | prepared→new_with_amount→build_transaction。 |
| lib / `unsigned_tests::unsigned_fields_and_canonical_wire` | `src/unsigned_tests.rs:18` | D | prepared→new_with_amount→build_transaction。 |
| bin / `preparation_cli_tests::preparation_mode_requires_all_explicit_fields` | `src/main.rs:364` | D | message()/Input::parseが転送messageまたはPreparedTransferを構築。 |
| bin / `tests::canonical_unsigned_transfer` | `src/main.rs:298` | D | message()/Input::parseが転送messageまたはPreparedTransferを構築。 |

### 今回実行したテスト一覧と結果

以下71件はすべて成功。追加9件を「追加」、前回からの再検証62件を「回帰」と表示。
完全なコマンド、個別ok行、libtest集計、最終チェックの生ログは
[実行ログ](phase-7z-safe-regression-execution.md)に保存。

| ターゲット / テスト完全名 | 区分 | 結果 |
|---|---|---|
| lib / `monitor::cli::tests::error_exit_code_redacted_json` | 回帰 | 成功 |
| lib / `monitor::cli::tests::height_and_config_json` | 回帰 | 成功 |
| lib / `monitor::cli::tests::invalid_address` | 回帰 | 成功 |
| lib / `monitor::cli::tests::invalid_tx` | 回帰 | 成功 |
| lib / `monitor::cli::tests::normal_balance` | 回帰 | 成功 |
| lib / `monitor::cli::tests::rpc_and_options_validation` | 回帰 | 成功 |
| lib / `monitor::cli::tests::timeout_exit_code` | 回帰 | 成功 |
| lib / `monitor::cli::tests::tx_absent_unknown` | 回帰 | 成功 |
| lib / `monitor::cli::tests::tx_confirmed` | 回帰 | 成功 |
| lib / `monitor::cli::tests::tx_failed` | 回帰 | 成功 |
| lib / `monitor::cli::tests::tx_unexecuted_unknown` | 回帰 | 成功 |
| lib / `monitor::cli::tests::zero_balance` | 回帰 | 成功 |
| lib / `monitor::query::tests::conflicting_duplicate_rejected` | 回帰 | 成功 |
| lib / `monitor::query::tests::detail_failure` | 回帰 | 成功 |
| lib / `monitor::query::tests::detail_id_mismatch` | 回帰 | 成功 |
| lib / `monitor::query::tests::detail_success_and_block_evidence` | 回帰 | 成功 |
| lib / `monitor::query::tests::duplicate_transaction` | 回帰 | 成功 |
| lib / `monitor::query::tests::empty_history` | 回帰 | 成功 |
| lib / `monitor::query::tests::explicit_pagination_requests` | 回帰 | 成功 |
| lib / `monitor::query::tests::finality_remains_unspecified` | 回帰 | 成功 |
| lib / `monitor::query::tests::history_shape_and_count_rejected` | 回帰 | 成功 |
| lib / `monitor::query::tests::invalid_inputs_no_query` | 回帰 | 成功 |
| lib / `monitor::query::tests::multiple_transactions` | 回帰 | 成功 |
| lib / `monitor::query::tests::nonadvancing_cursor_rejected` | 回帰 | 成功 |
| lib / `monitor::query::tests::null_is_not_absence_proof` | 回帰 | 成功 |
| lib / `monitor::query::tests::recorded_live_empty_history_shape` | 回帰 | 成功 |
| lib / `monitor::query::tests::rpc_errors_and_timeouts` | 回帰 | 成功 |
| lib / `monitor::tests::ambiguous_unexecuted_is_unknown` | 回帰 | 成功 |
| lib / `monitor::tests::balance_normal` | 回帰 | 成功 |
| lib / `monitor::tests::balance_zero` | 回帰 | 成功 |
| lib / `monitor::tests::duplicate_tx` | 回帰 | 成功 |
| lib / `monitor::tests::invalid_hash_before_rpc` | 回帰 | 成功 |
| lib / `monitor::tests::malformed_status_count` | 回帰 | 成功 |
| lib / `monitor::tests::no_requested_tx_is_not_empty_history` | 回帰 | 成功 |
| lib / `monitor::tests::recorded_live_status_remains_unknown` | 回帰 | 成功 |
| lib / `monitor::tests::rpc_error` | 回帰 | 成功 |
| lib / `monitor::tests::rpc_timeout` | 回帰 | 成功 |
| lib / `monitor::tests::tx_failed` | 回帰 | 成功 |
| lib / `monitor::tests::tx_success` | 回帰 | 成功 |
| lib / `monitor::tests::tx_unknown` | 回帰 | 成功 |
| lib / `monitor::tests::unknown_reasons_preserve_ambiguity` | 回帰 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::concurrent_handles_refused` | 追加 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::corruption::absent_directory_not_auto_initialized` | 追加 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::cross_process_lock_refuses_second_user` | 追加 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::integration::connection_failure` | 追加 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::missing_and_corrupt_files_refused` | 追加 | 成功 |
| lib / `pre_sign::lifecycle::persistent::tests::permissions_restricted_and_insecure_file_refused` | 追加 | 成功 |
| lib / `pre_sign::preparation::tests::invalid_inputs` | 追加 | 成功 |
| lib / `pre_sign::tests::invalid_fee_data_is_rejected_by_cdk_response_type` | 追加 | 成功 |
| lib / `unsigned_tests::invalid_or_missing_address_and_network_rejected` | 追加 | 成功 |
| bin / `monitor_cli_tests::explicit_monitor_and_existing_modes` | 回帰 | 成功 |
| bin / `testnet_tests::explicit_testnet_readonly_mode` | 回帰 | 成功 |
| bin / `testnet_tests::testnet_rejects_mutation_flags_and_invalid_address` | 回帰 | 成功 |
| bin / `tests::amount_override_is_rejected` | 回帰 | 成功 |
| bin / `tests::balance_boundaries` | 回帰 | 成功 |
| bin / `tests::inputs_fail_closed` | 回帰 | 成功 |
| bin / `tests::network_override_cannot_enable_sending` | 回帰 | 成功 |
| bin / `tests::ordinary_invocation_is_prepare_only` | 回帰 | 成功 |
| bin / `tests::send_flags_fail_before_rpc_or_key_access` | 回帰 | 成功 |
| test / `budget_retains_cursor_and_short_page_terminates` | 回帰 | 成功 |
| test / `cli_json_and_errors_reuse_existing_monitor` | 回帰 | 成功 |
| test / `conflicts_and_cursor_cycles_are_errors` | 回帰 | 成功 |
| test / `details_preserve_error_and_height` | 回帰 | 成功 |
| test / `invalid_inputs_do_not_call_rpc` | 回帰 | 成功 |
| test / `missing_and_incomplete_details_do_not_imply_success` | 回帰 | 成功 |
| test / `nonempty_history_preserves_provider_order_and_fields` | 回帰 | 成功 |
| test / `pagination_deduplicates_boundary_and_finishes` | 回帰 | 成功 |
| test / `rpc_errors_timeouts_and_empty_history` | 回帰 | 成功 |
| test / `synthetic_sdk_shapes_and_status_reasons` | 回帰 | 成功 |
| test / `typed_history_transaction_error_is_preserved` | 回帰 | 成功 |
| example / `tests::candidate_selection_is_bounded_and_explicit` | 回帰 | 成功 |

### 検証範囲・修正・残るリスク

TX状態/status_reason、詳細取得、アドレス履歴、ページネーション、重複排除、cursor循環拒否、
JSON、timeout、既存CLI互換は既存41件とPhase7Zの11件を再利用して再確認。
A/B全71件を完全名のallowlistで実行し、ログのok集合とallowlistの一致を機械照合。

実装不具合・Mock不具合・仕様不整合によるテスト失敗はなし。本体/Mock/fixtureの変更なし。
監査補助スクリプトの初回一覧解析で0件exampleの区切りを誤り、example名の照合assertが失敗。
テスト本体は成功していたが、集計の信頼性のため解析を修正し、全allowlistを再実行した。
その再実行分をユニーク件数へ重複加算していない。確定実行ログは修正後の1回分。

最終チェック: `cargo fmt --check`、`git diff --check`、
`cargo build --offline --locked`はすべて成功。Cargo.toml/Cargo.lock差分なし。
既存proc-macro-error2の将来互換警告のみ。依存追加・製品コード変更なし。
本作業の変更は本レポートの追記と実行ログ文書の追加のみ。
前回からのsrc/monitor/query.rs・tests/配下の変更、および無関係な未追跡
`docs/phase-7-signing-investigation.md`には変更を加えていない。

残る未検証はDの118件と実Testnet詳細/非空履歴/サーバcursor・保持期間・finality。
C/Dテスト未実行を成功扱いしない。分類は監査時のソース/fixture/環境に対して有効で、
共通fixtureや初期化が変われば再監査が必要。
次の候補は、TX構築・実署名と純粋な検証処理を分離したテスト設計の検討。
その変更は今回行わず、Mock化だけで実署名の暗号学的正当性まで証明できるとは主張しない。
外部通信、秘密鍵/Seedファイル読み取り、署名、TX作成/送信、Faucet、wallet接続、
ネットワーク設定変更、commit/push、Notion記録は行っていない。
