# Phase 7N — Testnet Transfer Preparation / Pre-Sign Validation

## 実装と入力

既存差分、Cargo依存、main/lib、PreparedTransfer、pre_sign_check、
7L/7M記録を確認。AGENTS.mdは対象/祖先で見つからなかった。
既存未コミット変更を維持した。依存追加なし。

新コマンド：
rialo-first --testnet-prepare-transfer --from PUBLIC_KEY --to PUBLIC_KEY --amount-kelvin INTEGER --fee-limit-kelvin INTEGER

すべて必須。fromは送付元兼fee payer、toは受取先。自動選択しない。
単位は明示的なkelvin。符号・小数・指数表記を拒否しu64で処理する。
1 RLO=1,000,000,000 kelvinは既存CDK constants.rsの定義。
入力は換算不要な最小単位整数のみ。
金額/上限とも正、合計overflow拒否。未知引数・重複・欠落も拒否する。

## 既存機能の再利用と変更範囲

- src/main.rs:25/113：引数解析、準備専用分岐、結果表示と終了コード。
- src/pre_sign/preparation.rs:12：入力検証。
- 同:120：既存check_with_rpc_and_clockを呼び、表示用観測値と追加条件を扱う。
  この関数は公開pre_sign_checkも利用する共通の検証本体。
- 同:171：固定TestnetReadRpcとSystemClockを利用する本番入口。
- src/lib.rs:59：PreparedTransfer::new_with_amountを追加。
  従来newは従来金額1,000,000を指定して委譲し、既存API互換性を維持。
  Builderの内部ヘルパーを共有し、金額をprivateフィールドに保持する。
- src/pre_sign.rs:281：残高計算を固定定数からPreparedTransferの金額へ変更。
- README：新コマンドの説明。

可変金額にしたのは準備用の構築と共通チェックのみ。
既存署名ライフサイクルは固定金額制約のまま。今回可変金額の実署名対応を
完成したとは扱わない。署名要求、ストア、送信経路へ接続しない。

## 準備チェック・手数料

入力時に既存PreparedTransferのアドレス・弱い送付元公開鍵・同一宛先チェックを再利用。
実行時は固定https://testnet.rialo.io/のconfig prefixを取得し、
実CDKで指定金額の未署名messageを構築する。
同じバイト列のhashを固定して既存pre_sign_checkの共通本体に渡す。

正式なCDK get_fee_for_message（rialo-cdk 0.18.1/src/rpc/client.rs:766）を使用。
VersionedMessageをbase64にした同一バイト列が見積もり対象。
RPC失敗・fee=None・fee>limitは停止し、上限で見積もりを代用しない。
実取得できた場合だけEstimated Feeに数値を表示する。

既存検証はmessage hash、prefix一致、照会前後prefix、fee、残高、
overflow、期限・時計を検査する。
追加で、取得したbalance >= amount + fee_limitを必須にする。
feeがlimitより低くてもこの条件を満たさなければ停止する。
追加の残高RPCをせず、共通チェックが観測した値を表示・判定に利用する。
30秒timeoutもBLOCKED結果として表示する。
各RPCは原子的スナップショットではなく、チェック後の状態変化を保証しない。

## 表示と安全性

Network/RPC/From/To/Amount/単位/Balance/単位/Fee Limit/単位/
Estimated Fee/Validation Result/Blocking Reasons/
Signature Status/Broadcast Statusを表示する。
取得不能値はUNKNOWN。入力不正時は検証済み入力がないため値をUNKNOWNとする。
Signature StatusとBroadcast Statusは常にNOT EXECUTED。
成功はPREPARATION PASSED、失敗はPREPARATION BLOCKEDと非ゼロ終了。

PREPARATION PASSEDはこの時点の準備条件成立のみ。
送金成功、鍵所有、宛先所有、実手数料上限、独立したchain IDを保証しない。
Network表示にもRPC config照合であってchain ID未証明と記載する。

準備モジュールのReadRpc能力はprefix/fee/balanceのみ。
署名者・broadcast・永続ストアを作成/呼び出す処理はない。
テストの表示確認だけでなく、この呼び出し構造で署名・送信経路から分離している。
既存CLIのDevNet準備とTestnet-readonlyモードは維持する。

## オフライン検証

既存120件＋新規9件＝129件成功、失敗0（library119、binary10）。
新規8 libraryテスト：
- 可変金額2,000,000 kelvin、正常アドレス、十分な残高で成功
- messageの実命令データに指定金額が入ることをMock fee処理で確認
- 不正from/to、同一アドレス、amount0、小数/負数、limit0、overflow、送信フラグ拒否
- amount+limit不足（amount+estimateは足りる境界を含む）
- amount+estimate不足
- fee上限超過、fee未取得、RPC失敗、config不一致
- 成功出力でも署名/送信はNOT EXECUTED

新規1 binaryテスト：
明示モードの解析、必須項目欠落・重複拒否。
1テスト内に複数ケースがあるため上記箇条書き数とテスト関数数は異なる。

cargo fmt --check、cargo check --offline --locked、
cargo test --offline --locked、git diff --check成功。
追加時の未使用import警告を除去。既存proc-macro-error2の将来互換性警告は残る。
不正入力で実バイナリを実行するオフライン確認も実施。
通常テストのMockはネットワーク接続しない。
既存120件にはメモリ内合成署名テストが含まれるが、今回の準備テストは署名しない。

## 未検証・次への条件

今回は新しい実ネットワーク操作を行っていない。
実Testnetでの可変金額の構築・手数料見積もり・準備コマンド全体は未検証。
Phase 7Lの残高0は過去観測であり、新たな残高取得と扱わない。
実署名者互換性、本番送信/結果確認、履歴巻き戻しは引き続き未解決。

次の最小作業はユーザー承認後の読み取り専用prepare実行。
事前にTestnet URL、送付元・宛先、amount/limit、公開message・balance/config/fee
の照会内容を提示する。残高不足ならBLOCKEDを正常な安全停止として記録する。
今回承認を得たと解釈せず、ネットワーク実行や次Phaseを開始しない。

実ウォレット操作・秘密鍵/seed読取・実署名・TX送信・Faucet・資産移動・
署名履歴変更・本番初期化・Git commit/push・Notion記録は0回。
