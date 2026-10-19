# Phase 7T — CLI / CDK Local Signing Compatibility Verification

## 1. 判定・現状（調査2026-10-17、再開確認2026-10-18 JST）

ローカル調査完了。安全なCLI未署名データ取得経路を確認できず、
CLI/CDKバイト比較は検証不能、互換性確認は未完了として停止する。
AGENTS.mdは対象・祖先・リポジトリ内に見つからなかった。
branchはmain、HEADはec38892（Record Phase 7G-7O preparation and stabilize
parallel persistence tests）。開始時のsrc/lib.rsの3行追加、未追跡の
unsigned_tests.rs、Phase 7P〜7S文書、phase-7-signing-investigation.mdは保持した。
今回の変更は本文書の新規作成だけ。本番コード・既存テストは変更していない。

## 2. CLI調査：実行コマンドと確認できた出力

実行したCLI操作は以下のversion/helpだけ。

```text
rialo --version
  rialo 0.18.1-2e1fdefe31cc
rialo --help
  Usage: rialo [OPTIONS] <COMMAND>
  --format <FORMAT> [possible values: human, json]
  --json  Output results in JSON format (shorthand for --format=json)
rialo client --help
  transfer  Transfer RLO tokens ... Sends the specified amount ...
  transaction  Retrieve and display detailed information about a transaction
rialo client transfer --help
  Usage: rialo client transfer [OPTIONS] <TO> <AMOUNT> [FROM] [PAYER]
```

transferの全オプション一覧にdry-run、simulate、build-only、unsigned export、
message出力、valid_from/config固定、手数料固定の専用オプションは見つからなかった。
JSONはコマンド結果の出力形式であり、署名前で停止する機能ではない。
--no-setupは初回セットアップ省略であり、署名・送信抑止ではない。
--verboseや--config-overridesで未署名出力できるとは推測しない。
ヘルプに載らない機能の不存在までは証明していない。

転送コマンドは送信操作と明記され、既存E2E記録にも署名・送信実績があるため、
実行禁止対象。airdropはFaucet要求、keytoolや宛先省略balance等も今回実行しない。
client transactionは既存TXのネットワーク照会であり、ローカル未署名構築ではない。
client一覧のdeploy、subscription作成、stake等の状態変更操作も未実行。
未知のフラグを試すためのtransfer起動や偽の鍵ファイルによる試行は行っていない。

Phase 7R:59以降のバイナリsymbolは探索の手掛かりに留まり、安全なexport APIの
証拠ではない。CLIの内部構築方法は引き続き未確認。
今回CLIから取得したのは版・引数仕様のみで、取引messageは取得していない。

## 3. CDK側の確認と根拠

Cargo.toml:7はrialo-cdk 0.18.1固定。
依存ソース基点：/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/

| 項目 | ファイル・行番号と確認結果 |
| --- | --- |
| PreparedTransfer | src/lib.rs:59。公開アドレス・金額・時刻・prefixを受ける |
| Builder | src/lib.rs:28/29。newとadd_transfer_instructionを使用 |
| 未署名構築 | src/lib.rs:83/84。build_transaction後、:85でVersionedMessageへ復号し再シリアライズ一致を検査 |
| 署名対象取得 | src/lib.rs:107、transaction.message()のraw bytes |
| CDK API | rialo-cdk-0.18.1/src/transaction/builder.rs:529、Result<Transaction>、ゼロ署名枠。:107 message() |
| message_data | rialo-s-transaction-0.18.1/src/lib.rs:666、別SDK型でmessage().serialize()を返す |
| 署名対象範囲 | CDK builder.rs:488、build()のmessage bytesを署名へ渡す。今回は実行せず |
| message形式 | 同:645。header、account配列、valid_from LE i64、config prefix LE u64、OCC、命令配列 |
| wire形式 | 同:705。compact署名配列とmessage。ゼロ枠のwireは署名済みTXではない |
| TX ID | CDK rpc/client.rs:524/545。RPC返却文字列をSignatureとして扱う。CLI/ノードの算出規約は未証明 |

署名対象SHA-256やゼロ署名枠をTX IDと呼ばない。
このtransfer形式に通常のrecent blockhash欄があるとは推測しない。
比較にはfrom/to/payer、整数kelvin金額、valid_from、config prefix、OCC、
必要署名数、命令とaccount順序を一致させる必要がある。
手数料見積もり上限はアプリ側の条件で、message内の強制上限ではない。
CLIに時刻/config等を固定する確認済み手段がないため、同一入力比較はできない。

## 4. 比較・テスト結果

CLI対CDKのmessage長、バイト列、wire形式、署名範囲、TX ID条件は全て未検証。
一致を確認したCLI/CDK項目0、不一致を確認した項目0。
「不一致0」は一致の証明ではない。CLIデータを得ていないので比較自体が未実施。

既存src/unsigned_tests.rsを読み、秘密鍵・署名・RPC・永続化操作がないことを
確認して `cargo test --offline --locked unsigned_tests` のみ実行した。
5件成功・0件失敗、119 library＋10 binaryがfilterで未実行。
構成は合計134件。今回全件成功とは報告しない。削除・無効化・変更なし。

- src/unsigned_tests.rs:18：公開データによる未署名構造、wire往復一致。
- :48：CDK同一入力の再現性、金額・時刻・prefix・宛先変更の反映。
- :73：messageと署名配列付きwireの区別、canonical serialization。
- :89：不正wire拒否。
- :99：無効入力拒否。

この一致はCDK内部の再現性で、CLI互換性ではない。テスト追加0件。
proc-macro-error2 v2.0.1の既存将来互換性警告あり。
git diff --check成功。今回Web検索・外部RPCアクセスなし。

## 5. 未解決事項・最小の次作業

CLIの安全な未署名export、同一入力固定、正式fixtureが不足している。
実送金には別途、正式な実署名者接続・公開鍵照合、ネットワーク有効期間、
手数料0の原因確認、最新残高/見積もり、送信/結果確認経路、二重送信防止、
明示的実行承認が必要。互換性確認だけで実送金可能とはしない。

次は配布元に「0.18.1で鍵・RPCを使わず、時刻/configを固定して未署名messageを
出力する正式手順、または同条件の公式fixture」を照会するための情報を揃える。
必要入力は上記表の直後に列挙した項目。今回は外部照会を送信していない。
資料が得られるまで同じヘルプ/Web探索や重複テストを続けず停止する。

実ウォレット操作、秘密鍵/seed読取、署名、送金、Faucet、署名状態変更、
commit/push、Notion記録は0回。Phase 7Uは開始しない。

## 6. 中断後の再開・最終確認（2026-10-18 JST）

中断時点で本書のCLIヘルプ調査・CDK仕様整理・未署名5件のテストは完了していた。
再開時にAGENTS.mdの有無、git status/diff、Phase 7P〜7S、本書、既存コードと
テスト構成を確認した。既知のCLIヘルプ探索・Web検索は繰り返していない。
残項目だったfmt/checkと結果記録を実施し、未署名5件も再確認した。

| 確認 | 今回の結果 |
| --- | --- |
| cargo fmt --check | 成功 |
| cargo check --offline --locked | 成功 |
| cargo test --offline --locked unsigned_tests | 5成功、0失敗 |
| cargo test --offline --locked（無指定の全件） | 未実施。下記の署名禁止との衝突による |
| git diff --check | 成功 |

既存全件にはsrc/lib.rs:154、src/pre_sign/lifecycle.rs:372等で合成鍵による
署名を行うテストが含まれる。今回のSTEP 2の「署名」禁止と公開データだけの
検証範囲を優先し、全件コマンドは実行していない。残り129件は今回未検証。
テストの削除・無効化・コード変更はしていない。新規テスト0件。
proc-macro-error2 v2.0.1の将来互換性警告1種類は継続している。

安全なCLI未署名取得経路は「存在しない」とは断定できず、「確認できない」が結論。
根拠は本書:13以降の既存ヘルプ調査と:48以降のAPI確認。
CLI側データがないため5つの比較項目はすべて未検証で、不一致の証拠もない。

調査と文書の仕上げは完了。Phase 7T全体は、指定の全件テスト未実施を残すため
未完了と報告する。CLI互換性も未検証。禁止条件を緩めて完了扱いにはしない。
次の最小作業は、対象CLI版の鍵・RPC不要な未署名出力手順または公式fixtureの
提供を配布元へ照会すること（今回は送信しない）。全件テストの再実行には、
実ウォレットを使わないメモリ内テスト署名を許可するかの明確化が必要。
今回更新したのは本書のみ。既存作業を保持して停止する。
