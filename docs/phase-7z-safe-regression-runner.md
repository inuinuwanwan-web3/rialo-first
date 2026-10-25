# Phase 7Z — Safe Offline Regression Runner

判定：**PASS（監査済みの安全なオフライン回帰Runnerに限定）**。
実TestnetのTX詳細・非空履歴取得成功を意味しない。新しいPhaseは開始していない。

## 根拠と実装

基準commitは172aa6a（Phase 7Z監査）。ユーザー指定の
`docs/phase-7z-regression-execution.md`は存在しないため、既存の
`docs/phase-7z-safe-regression-execution.md`と
`docs/phase-7z-fixture-monitor-validation.md`を使用した。SDK仕様は再調査していない。

- `scripts/phase7z_allowlist.json`：監査レポートから189件のinventoryと許可71件を明示列挙。
  識別子はCargoターゲット種別＋完全修飾名。既存のテスト定義は変更していない。
- `scripts/phase7z_regression.py`：Python標準ライブラリと既存OSのlibseccompを使用。
  Cargo/Rust/Pythonの依存追加やパッケージインストールは行っていない。
- `scripts/test_phase7z_regression.py`：Runner自体の9テスト。既存189件とは別に集計。
- `docs/phase-7z-safe-regression-runner-results.json`：日時、通信遮断方式、各テストの
  結果/所要時間/出力/exit code、未実行一覧と理由を保存。

許可リストを監査レポートの成功71件と毎回比較する。Cargo.toml/Cargo.lock、
src/tests/examplesの全ファイル集合とSHA-256、元の監査文書のSHA-256も固定。
ソース変更・追加・不一致・未監査Cargo設定/build.rsは実行前に拒否し、再監査を要求する。
許可外の明示指定、重複指定、D指定も拒否。許可リストを自動拡張しない。

## OSレベルの通信遮断

Linux seccompフィルタをRunnerへロードし、fork/exec先のCargo・rustc・テスト・孫プロセスにも継承。
新規socket、connect、bind/listen/accept、io_uringをEPERMで拒否。
libseccompがない、必須syscallの解決不能、filterロード不能、遮断probe失敗のいずれも
テストを実行せずblocked JSONと非ゼロ終了を返す。

probeはIPv4/IPv6/Unix socket作成がEPERMになることを確認し、外部アドレスには接続しない。
Runner自己テストでexec先でも同じ拒否を確認。無効FDに対するconnectもEPERM、
非Unix socketpairもEPERMになることを確認した。
Cargoのjobserver/子プロセス制御に必要な匿名AF_UNIX socketpairだけを許可する。
匿名pairはローカルIPCで、外部接続先を指定できない。名前付きUnix socket作成やconnectは拒否。
子プロセス起動時はclose_fds=Trueで余分な既存FDも継承しない。

ネットワーク名前空間だけでは当環境で遮断を検証できなかったため、上記の明示的な
seccomp制限を採用。既存のネットワーク設定やAGP監視プロセスは変更していない。

## 実行手順

前提：Linux、Python 3、既存libseccomp.so.2、Rust/Cargo、ローカルキャッシュ済み依存。

```bash
python3 -B scripts/test_phase7z_regression.py
python3 -B scripts/phase7z_regression.py --output /tmp/phase7z-regression-new.json
```

結果ファイルは新しい名前を指定する。既存結果は上書きしない。

許可済みの単一テストを実行する例：

```bash
python3 -B scripts/phase7z_regression.py \
  --output /tmp/phase7z-one-new.json \
  --test lib:monitor::tests::tx_success
```

通信遮断・監査照合後に`cargo test --offline --locked --all-targets --no-run --message-format=json`
でコンパイルのみ実施。取得したtest executableの`--list`を189件と照合した後、
各許可テストを`--exact`で1件ずつ実行する。通常のexample mainやDのテスト本体は実行しない。
ビルド既定300秒、1テスト既定30秒（正の有限値のみ）。
`--build-timeout`/`--test-timeout`で指定可能。
タイムアウト/中断時はRunnerが起動した専用process groupだけをkillしてwaitし、
以降のテストを止める。結果行の不足・重複・想定外テスト・失敗・ignoredも成功扱いしない。
失敗/blocked時は終了コード1、成功時0。引数不正はargparseの終了コード2。

子実行環境は必要なPATH/Rustup設定等だけを渡す。新規temp HOME/CARGO_HOMEを使い、
既存Cargoのregistryキャッシュだけを参照する。credentialsやwalletのディレクトリは
CARGO_HOMEへコピーしない。継承したRPC設定/認証情報/テスト用分岐環境変数は渡さない。
保存系テスト用TMPDIRも新規の専用temp rootに限定し、終了時に削除する。
環境変更は子プロセスのみに適用し、既存設定を変更しない。

## 最終実行結果

実行UTC：`2026-10-25T06:03:13.050391+00:00` ～ `2026-10-25T06:04:17.502515+00:00`。

| 対象 | 総数 | 成功 | 失敗 | 未実行 |
|---|---:|---:|---:|---:|
| 監査済みRustテスト | 189 | 71 | 0 | 118 |
| Runner自己テスト | 9 | 9 | 0 | 0 |

Rust成功内訳：lib 50、bin 9、integration 11、example 1。
許可リスト71件＝実行結果71件を集合/件数で照合。JSONの189＝71＋118も確認。
全189件の分類はA18/B53/C0/D118。前回未実行127件の分類A8/B1/C0/D118とも一致。
今回の「追加機能」はRunnerであり、既存189件にRustテストを追加したものではない。
前回71件・今回71件・開発中の再実行を足して成功数を水増ししない。

未実行118件はすべてD。JSONにテスト名・分類・excluded_by_auditを記録。
TX生成、SigningKey操作、実署名を共通fixtureから呼ぶため、Mock通信でも実行しない。
理由の個別ソース位置は既存の監査文書を参照。

許可外をCLI指定した負の検証は、終了コード1、blocked、実行0、未実行189のJSONを確認。
Runner自己テストの合成失敗/timeoutは意図的に注入した制御フロー検証であり、
Rust回帰の失敗件数には加算しない。フィルタ不能時にはcommandが呼ばれないことも検証。

開発途中の修正：当初socketpairをすべて拒否してCargoのコンパイルが停止したため、
匿名AF_UNIX pairだけを許可するルールへ修正。外部socket/connect禁止は維持。
初回自己テストの子Python文字列の構文不備も修正し、最終9件は全成功。
既存の安全なRustテストに不具合や失敗は見つかっていない。

`cargo fmt --check`、`git diff --check`は成功。
`Cargo.toml`、`Cargo.lock`、既存src/tests/examples、監査文書には差分なし。
既存依存proc-macro-error2の将来互換警告は残るが、コンパイルとテストは成功。

### Runner自己テストログ

```text
test_allowlist_matches_audit_and_rejects_unknown (__main__.RunnerTests.test_allowlist_matches_audit_and_rejects_unknown) ... ok
test_changed_source_fails_closed (__main__.RunnerTests.test_changed_source_fails_closed) ... ok
test_json_never_overwrites (__main__.RunnerTests.test_json_never_overwrites) ... ok
test_network_filter_inherits_to_exec (__main__.RunnerTests.test_network_filter_inherits_to_exec) ... ok
test_network_unavailable_runs_no_tests (__main__.RunnerTests.test_network_unavailable_runs_no_tests) ... ok
test_result_parser_rejects_failed_missing_and_extra (__main__.RunnerTests.test_result_parser_rejects_failed_missing_and_extra) ... ok
test_test_failure_stops_and_returns_nonzero_json (__main__.RunnerTests.test_test_failure_stops_and_returns_nonzero_json) ... ok
test_test_timeout_stops_and_records_json (__main__.RunnerTests.test_test_timeout_stops_and_records_json) ... ok
test_timeout_kills_owned_process_group (__main__.RunnerTests.test_timeout_kills_owned_process_group) ... ok

----------------------------------------------------------------------
Ran 9 tests in 0.740s

OK
```

## 残るリスク・次の候補

- 実Testnetの詳細/非空履歴/サーバcursor/保持範囲/finalityと、D118件は未検証。
- このRunnerは監査済みソースを信頼する回帰機構であり、任意の悪意あるコードを
  安全に動かす汎用filesystem sandboxではない。HOME隔離と監査hashを併用するが、
  ホスト全体のファイル読み取りをseccompで遮断するものではない。
- インストール済みtoolchain、libseccomp、registryキャッシュの完全性はホストの信頼境界。
  ソース検証とビルドの間に別プロセスが変更する環境では実行しない。
- 新規temp CARGO_HOMEを使うためキャッシュの指紋が変わり再コンパイルする場合がある。
  ビルドが時間制限を超えた場合も制限を緩めず停止する。
- 許可リスト/hash更新は自動化せず、変更ソースと新テストの再監査後に行う。
  次の候補は、専用CIで同じ遮断下実行を行う運用設計と、監査更新手順の整備。

秘密鍵/Seed取得、wallet署名、TX作成/送信、Faucet、実RPC/外部API、AGP干渉、
commit/push、Notion操作は行っていない。GitHub/Notion記録は今回の禁止事項に従って未実施。
