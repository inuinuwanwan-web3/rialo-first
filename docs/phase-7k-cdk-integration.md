# Phase 7K — Rialo CDK Offline Application Integration Test

## 現状調査と判定

既存コード、Cargo.toml/Cargo.lock、README、Phase 7G〜7J文書、Git差分を確認。
適用対象ツリーと祖先にAGENTS.mdは見つからなかった。
既存の未コミット変更を保持。本番署名・永続化機構は変更していない。
今回は既存ライブラリ経路のオフライン統合検証まで完了。
CLIアプリ全体のTestnet E2Eは未完成。一過性の既存テスト失敗も未解決。

main.rsの実行経路はDevNet読み取り準備用。
lib.rs以降のTestnet向け外部署名ライフサイクルとは未接続。
CLIで過去に成功したTestnet送金は、このRustアプリの送信実績ではない。
HTTP接続、公開鍵の実ウォレット取得、実署名、実送信をMock成功で代替認定しない。

## 機能別の現在地

A=実装済み・オフラインテスト成功、B=実装済み・実経路未検証、
C=一部実装、D=未実装。Aは実ネットワークでの成功を意味しない。

| 機能 | 分類 | 根拠・不足 |
| --- | --- | --- |
| CDK HTTPクライアント初期化 | B | main.rs:69、pre_sign.rs:155。コンパイル確認、今回実接続なし |
| RPC接続・高さ・config取得 | B | main.rs:69はDevNet。pre_sign.rs:215のTestnetアダプタも実RPC未検証 |
| ウォレット公開鍵取得 | C | main.rs:22は引数解析。テスト鍵から取得はintegration.rs:97。実ウォレット取得はD |
| 残高・手数料取得 | C | pre_sign.rs:215、:247。既存検証ロジック＋MockはA、HTTP経路はB |
| CDKトランザクション構築 | A | lib.rs:13、:38。実CDK Builderを使用 |
| 署名・署名応答検証 | C | lib.rs:83、lifecycle.rs:123の検証・wire構築はA。実署名者接続はD |
| 署名状態保存・復元 | A | persistent.rs:120、:146、:201、:334。喪失/巻き戻し耐性は7Jの制限あり |
| TX送信 | D | 本番send_transaction呼び出しなし。integration.rs:44はテスト専用 |
| TX結果確認 | D | 本番get_signature_statuses/get_transaction接続なし。integration.rs:61は模擬結果 |
| エラー処理 | C | main.rs:100、diagnostics.rs:80、pre_sign.rs:155。既存拒否経路はA、送信結果不明の本番処理はD |

## 統合テスト

追加ファイル：
src/pre_sign/lifecycle/persistent/tests/integration.rs
既存persistent.rs:372にcfg(test)配下のモジュール宣言だけ追加。
docs/phase-7k-cdk-integration.mdを新規作成。

既存のprivate ReadRpcとvalidate_usingを子テストモジュールから利用するため、
本番APIを公開・緩和する必要はなかった。
これはライブラリ内での統合テストであり、実CLIバイナリのE2Eではない。
テスト専用Appが不足する送信・結果処理を明示的に模擬する。
App.connectはエラー注入点であり、実HttpRpcClient初期化の試験ではない。

実行順：
1. テストApp初期化とMock接続確認
2. メモリ内の合成Ed25519鍵から公開鍵取得
3. 実CDK PreparedTransferで送金メッセージ構築
4. 同じメッセージでMock手数料取得・残高確認
5. 既存pre_sign_check相当と再検証を実行
6. 既存PersistentLifecycleで要求を取得
7. 合成鍵で正確なメッセージを署名
8. 既存署名検証・wire生成とSIGNED永続保存
9. 模擬送信側でCDKのdeserialize_wiredを実行し、
   メッセージ一致・署名数・署名の正当性を再確認
10. 模擬TX IDと成功/失敗/不明/タイムアウトを処理
11. ストアをdrop/openし、保存状態復元と同一ID再登録拒否を検査

残高と手数料は初回・直前再検証の2回取得することを確認。
実装の順序に合わせ、構築後に同一メッセージの見積もりと残高を確認する。
保存成功前に模擬送信へ進まないことをイベント順序で検査する。
成功時でも保存データは署名状態のみ。TX IDや実行結果を永続化する
本番フローが完成したわけではない。

今回の復元は同一プロセス内のclose/open。
実プロセス終了・再起動は既存Phase 7HのSIGKILL＋別プロセス試験を再実行した。
アプリ全体の別プロセスE2Eを新規実装したとは扱わない。

## 正常系・異常系12件

integration.rs:195以降。
正常、接続失敗、残高不足（送金額は足りるが手数料込みでは不足）、
手数料未取得、見積もり上限超過、不正宛先による構築失敗、
ダミー署名者失敗、実writeエラーによる保存失敗、
模擬送信失敗、結果不明、結果取得タイムアウト、模擬実行失敗。

- 署名前の失敗は模擬送信0回。
- 保存失敗はテストjournalの読み取り専用ハンドルで実writeエラーを起こす。
  保存済み要求は保持され、署名済みwireを送信しない。
- 模擬送信失敗/結果不明/タイムアウトでも送信回数は1回。
  自動再送・SIGNED初期化なし。再読み込みでも同一IDを拒否する。
- 署名者失敗後はAWAITING_SIGNATUREが残るが、再登録は許可しない。
- 模擬実行失敗と結果不明は別エラーとして扱う。
- 原本の読み込み前後でjournalバイト列が不変であることを検査する。

## 実行結果・発見事項

初回の既存テスト並列実行：
library 98成功・1失敗、binaryは未実行。
duplicate_and_different_message_id_refusedの再openでLockUnavailableが発生した
（persistent.rs:545付近）。ロックは安全側に取得を拒否した。

変更前の逐次実行：既存106件すべて成功。
追加後の指定コマンド cargo test --offline --locked：
既存106件＋新規12件＝118件成功、0失敗（library111、binary7）。
cargo fmt --check、cargo check --offline --locked、git diff --checkも成功。

一過性ロック失敗の根因は未確定。並列の子プロセス生成との関係等は仮説であり、
修正済みとは扱わない。ロックのリトライや無視、製品の条件緩和は追加していない。
今回の統合経路で新たな製品不具合は確認しなかったが、
CLIとライブラリのネットワーク・実行経路の分離がBuilder実践の障害と分かった。
安全性に影響する製品修正は実施していない。

## warnings

cargo check/testとcargo report future-incompatibilities --id 1で
確認できた原因は1種類：
proc-macro-error2 2.0.1/src/lib.rs:494の非公開proc_macro再公開（E0365）。
現在は成功するが将来Rustでエラー化する可能性があり、依存更新の検討が必要。
今回のテスト追加に起因しない。画面に残る「7件」の全内訳は再現できず、
残り6件の存在・原因・解消は未確認。推測で一覧を作らない。

## Testnet移行条件と最短経路

1. 次の具体的作業はTestnet専用prepare-only CLIへ既存ライブラリを接続すること。
   main.rsのDevNet準備経路を明示的に整理し、同じPreparedTransferを
   表示・見積もり・要求生成に使う。まずモック注入でCLIの一連実行を検証する。
   今回はこの製品変更を開始していない。
2. ユーザーが外部通信を許可した後、署名なしでTestnet RPCのconfig prefix、
   残高・メッセージ手数料・時刻/期限を確認する。
3. 正式にメッセージ署名できる署名者を決める。
   CLI alias defaultとの互換性は未確認。秘密鍵の取り出しを代替策にしない。
   公開鍵・正確な署名対象・宛先・金額・期限の一致を確認する。
4. 本番送信・結果確認のアダプタを実CDK APIに基づき実装する。
   署名済み状態とTX IDを対応させ、送信応答喪失時は再送せず調査へ止める。
   新規模擬経路だけで本番アダプタの動作確認を省略しない。
5. 署名要求と送信内容を提示し、実ウォレットアクセス・署名・Testnet送信について
   ユーザーの明示的承認を受けてから、限定した一回のE2Eを行う。
   Faucetが必要ならそれも別途許可を得る。
6. 送信signatureに対するstatusとtransactionの実行結果を照合し、
   成功条件（実行済み・エラーなし）を確認する。結果不明なら停止する。

模擬値は送金1,000,000 kelvin、手数料5,000、見積もり上限6,000。
5,000は実Testnetの現在価格を保証しない。
将来は同じVersionedMessageでgetFeeForMessageを呼び、
amount + fee <= balanceと見積もり上限を確認する。実手数料の上限保証ではない。

7G〜7Jの保全・停止機構は保持するが、永続基盤の追加拡張を次の目的にしない。
最も価値があるのは、公開鍵入力からCDKメッセージと実見積もりを一貫して
確認できるTestnet向け準備アプリ。実署名・送信はその後の明示承認で進める。
継続署名サービス化する場合の履歴喪失/巻き戻しリスクは未解決のまま残る。

## 安全条件・終了

全テストは一時ディレクトリとメモリ内合成鍵だけを使用。
実ウォレットアクセス・実ウォレット署名・TX送信・Faucet・外部通信・
本番保存変更・履歴削除/初期化・Git commit/push・Notion記録は0回。
本番データの修復や初期化は行っていない。
Phase 7Kのオフライン統合検証で停止。次Phaseは開始しない。
