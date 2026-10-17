# Phase 7M — Rialo Testnet Transaction Readiness

## 1. 判定・調査範囲

調査と変更案の整理は完了。CDK実送金の準備は未完了。
正式な実署名者接続、実送信/結果確認のアプリ統合、最新残高/手数料確認が不足。
今回はコード変更・新規Mock追加なし。本書だけ作成する。

AGENTS.mdは対象と祖先に見つからなかった。
Git差分、Cargo.toml/lock、README、既存CDKソース、Phase 7Lと過去E2E、
署名ライフサイクルを確認し、未コミット変更を維持した。
CLIはヘルプだけ参照。keytool address/listの実行や設定ファイルの読み出しは
内部で鍵を読む可能性を排除できないため行わない。現在のdefault再照合は未検証。
CLIの既存設定は過去の公開記録とヘルプで確認できる範囲に限定した。

ネットワーク再照会は今回は不要と判断し実行しなかった。
残高・ネットワーク情報はPhase 7L時点の観測で、今回の最新値ではない。

## 2. ウォレット対応

| 区分 | 公開アドレス | ネットワーク・役割 | 署名方式・CDK利用 |
| --- | --- | --- | --- |
| A 過去のCLI default | 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa | 過去Testnet送金元・fee payer | CLIが署名した記録あり。鍵保存形式/alias解決の実装は未確認。CDK署名者として未接続 |
| B Playground | FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6 | 過去Testnet受取先、ユーザー指定custodial wallet | 過去の受信に署名不要。Playgroundの署名API・鍵管理・CDK署名利用は未確認 |
| C Phase 7Lの0 kelvin | 7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa | Aと同じ公開アドレスをTestnet RPCで照会 | 読み取り成功は鍵所有/署名利用の証明ではない |

根拠：docs/2026-10-11-testnet-transfer-e2e.md:74/78、
docs/phase-7l-testnet-readonly.md:53。
A=C、A≠B。Cの0残高はBの残高消失を意味しない。
Bの現在残高は未照会。Aの過去9,995,000 kelvinと7Lの0の差の原因も未確認。
公開アドレスだけで残高照会や宛先指定はできるが、送付元として署名できるとは限らない。

## 3. CLI経路とCDKの対応

今回確認したCLIヘルプ：
- rialo --help：--alias、--network、--url。URLはnetworkより優先。
  alias省略時はcurrent_account、初期設定はdefaultと記載。
- rialo client transfer --help：
  transfer <TO> <AMOUNT> [FROM] [PAYER]、custom sender keypair指定の説明。
  手数料上限オプションはこのヘルプにない。
- rialo keytool address --help：--aliasまたは--keypair。
  公開鍵表示コマンドでも鍵読み取り不要とは証明できないため実行していない。
- rialo transfer --helpは未知のsubcommand。実送金コマンドは実行していない。

CLI本体ソースは今回検索したCargo registry、インストール先に見つからない。
したがってCLI内部のBuilder呼び出し、鍵形式、署名コード、preflight/retry、
RPCオプションの正確な値は未検証。CDKと同じコードを使うと推測しない。

| 項目 | CLIで確認できたこと | CDKで再利用可能な部分 |
| --- | --- | --- |
| TX構築 | 過去にSystem転送成功、内部実装は未確認 | TransactionBuilder/add_transfer_instruction |
| 署名アルゴリズム | CLI本体の実装確認は未完了 | CDK Ed25519署名検証と正確なmessageの公開 |
| 署名者指定 | --alias、FROM/PAYER、keypairのヘルプ | CDK Keyring indexまたは外部署名。aliasとの対応は未証明 |
| 手数料 | 過去5,000 kelvin、指定方式は未確認 | 同一messageのgetFeeForMessage |
| 宛先/ネットワーク | TO、--network/--url | 公開宛先検証、固定TESTNET_RPC |
| RPC送信 | 過去の送金成功記録。内部経路未確認 | CDK send_transaction APIは存在、アプリ未接続 |
| TX ID | 過去記録にsignatureあり | CDKはRPC応答文字列をSignatureへ変換 |
| 実行確認 | 過去statusとtransactionで成功確認 | 両照会APIは存在、アプリ未接続 |

過去の実績はdocs/2026-10-11-testnet-transfer-e2e.md:72/94。
その成功をCDKアプリの送金成功と扱わない。

## 4. CDKアプリの現在地

A=実装済み・記載範囲で検証済み、B=実装済み・実経路未検証、
C=一部実装、D=未実装。

| 機能 | 分類 | 根拠と検証範囲 |
| --- | --- | --- |
| RPC接続 | A | src/main.rs:79、7Lの実Testnet読み取り |
| 公開鍵 | C | main.rs:28の公開アドレス解析、テスト鍵取得は検証済み。実ウォレット照合未完 |
| 残高 | A | main.rs:93付近、7LでAの0 kelvin取得 |
| TX構築 | A | src/lib.rs:13/38、CDK Builder・canonical messageのオフライン検証 |
| 手数料取得 | B | pre_sign.rs:215/247、実アダプタあり。Mock検証済み、現Testnetの見積もり未確認 |
| 署名者接続 | D | lifecycle.rs:21/39に要求応答型、lib.rs:83に検証はあるが実署名者なし |
| 署名状態永続化 | A | persistent.rs:120/146/201/334、オフラインとクラッシュ試験。巻き戻し保証なし |
| TX送信 | D | アプリの本番呼び出しなし。integration.rs:44はMockのみ |
| TX結果確認 | D | integration.rs:61はMockのみ。本番アダプタ未接続 |
| エラー処理 | C | main.rs:79、diagnostics.rs:80。既存経路は検証済み、実送信例外は未接続 |
| 二重送信防止 | C | persistent.rs:201は同一ID拒否、7Kは自動再送なし。本番送信結果・TX IDの永続管理は未実装 |

## 5. 正式なCDK署名・TX仕様（コード確認、実署名なし）

以下のCDK参照ルート：
/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rialo-cdk-0.18.1/

- src/keyring/traits.rs:24/110：ed25519_dalekのSigningKey/VerifyingKeyとSigner。
  Ed25519で渡されたmessageバイト列そのものを署名する。
- src/keyring/file.rs:326：KeyringProvider::load(name,password)。
  保存keypairsを読み、encryption feature時は復号する。
  同:338以降の非暗号化分岐では32バイトをSigningKeyとして扱う。
  これはCDKの形式で、CLI alias=Keyring名、default=index0とは証明できない。
- src/transaction/builder.rs:488：sign_with_keypairはbuildのmessageを署名してwire化。
- 同:529：build_transactionは未署名Transaction。
- 同:107：message()で署名対象を取得。
- 同:66：add_signature_for_pubkeyで署名者位置へ組み込む。
- 同:38：正式なメソッド名はserialize_wired（serialize_wireではない）。
- アプリsrc/lib.rs:77/83：messageを取り出し、Ed25519 strict検証、
  公開鍵一致・署名数を確認してserialize_wiredを呼ぶ。

アプリで使用する公開鍵は32バイトのBase58表現、外部署名応答はraw64バイト。
署名対象はbase64文字列やmessage hashではなく、CDK生成の正確なmessage。
wireは署名配列とmessageから成る。今回の単一署名ケースは
署名数1＋64バイト署名＋messageとして既存テストで照合済み。
messageの送付元・宛先・金額・valid_from・config prefixを再検証する。

既存依存rialo-cdk、rialo-s-message、ed25519-dalek、bincode、base64、
sha2、tokioでこれらは構成済み。新規ライブラリは今回不要。
CLI鍵形式・Playground署名APIとの互換性は未検証のまま。

## 6. 手数料・送信・結果確認

CDK src/rpc/client.rs:766 get_fee_for_messageはbase64化した
VersionedMessageをgetFeeForMessageへ送る。
アプリpre_sign.rs:247以降は正確なmessageとhashを確認し、
feeの存在、見積もり上限、amount+feeのoverflow、残高不足を拒否する。
単位はkelvin、1 RLO=1,000,000,000 kelvin。
サーバーの現在の料金計算式や将来変動は今回確認していない。
過去5,000 kelvinを恒久料金とせず、署名前に同一messageで再見積もりする。
この上限チェックは実際に徴収される手数料の上限保証ではない。

CDK src/rpc/client.rs:524 send_transaction：
wireをbase64化してsendTransactionへPOSTし、応答をSignatureへ変換。
デフォルトはskip_preflight=true、max_retries=0、wait_for_execution=false。
重要：rialo-api-types-0.18.1/src/messages/send_transaction.rs:11以降には
encoding/wait_for_executionのみ実装、skip_preflight/max_retries/min_context_slot
は無視される旨の記載がある。現在サーバーの実挙動は未検証。
将来の変更案では明示オプションを指定しても、preflight保証と見なさない。
アプリ側の送信回数制御が必要で、再試行禁止をサーバー設定任せにしない。

CDK src/rpc/client.rs:619 get_signature_statusesは
Vec<Option<SignatureStatus>>、:586 get_transactionはtransaction/metaを返す。
API型get_signature_statuses.rs:57以降にexecutedとerrがある。
成功判定案：対象signatureのexecuted=trueかつerrなし、
get_transactionのmeta.errなし、期待する取引内容・金額・署名との一致。
エラーありは失敗、未実行/null/取得失敗/timeoutは成功扱いせず結果不明。
RPCからsignatureが返るだけでは実行成功ではない。
CDKのstatus変換はcontext.slotを使用するため、それを取引実行slotと断定しない。

送信応答喪失時に再署名・再送せず停止する。
署名済みTXから照会用signatureを確実に識別できるよう、
送信前にrequest_idとTX識別情報を結び付ける設計が必要。
SIGNEDは実行成功を意味しない。現在のジャーナルはTX送信・実行履歴ではない。
既存のID拒否は失われた履歴や別IDでの同一送金まで防ぐものではない。

## 7. 不足機能と具体的変更案（未実装）

既存で再利用：Testnet HTTP接続、公開アドレス検証、PreparedTransfer、
pre_sign_checkと期限再検証、外部署名検証・wire化、署名状態永続化。

次の最小作業：
Testnet prepare-onlyコマンドを既存PreparedTransfer/pre_sign_checkへ接続する。
mainの従来--from/--toはDevNetのままなので、明示Testnetモードを設ける。
公開送付元・宛先、固定0.001 RLO、最新config/時刻から未署名messageを構築、
同じmessageで手数料と残高を照会し、不足なら理由を表示して停止する。
署名・送信・ストア初期化は含めない。既存機能を再実装しない。

その次に必要：
1. CLI 0.18.1-2e1fdefe31ccのalias/鍵解決コードか正式な外部署名仕様を確認。
   認可された署名者経路を確定するまで、鍵の読み込みや変換を実装しない。
2. 署名要求/応答の実トランスポートを選び、公開鍵とメッセージを照合する。
3. 送信前のTX識別・結果不明時の停止と照会を設計し、本番send/status/transaction
   アダプタを既存ライフサイクルへ接続する。今回これらは実装しない。
4. 最新ネットワーク・資金・見積もりを確認。Aは7L時点で0なので現状の資金充足を
   仮定しない。Bの資金を無断で使う案に置き換えない。
5. 送付元・宛先・金額・見積もり・操作範囲を提示し、
   実署名/送信/必要な資金準備は別途承認後にのみ行う。

実送金の準備が整っていないため、次Phaseで送金実行を提案する段階ではない。
永続化基盤のさらなる拡張より、上記prepare-only統合と正式署名者の特定を優先する。

## 8. 検証・安全条件

cargo fmt --check、cargo check --offline --locked、
cargo test --offline --locked、git diff --check：成功。
既存120件成功（library111、binary9）、失敗0、新規テスト0。
既存テストのメモリ内合成署名だけを使用。実ウォレット署名は0回。
警告はproc-macro-error2 2.0.1の将来互換性E0365：
src/lib.rs:494の非公開proc_macro再公開。現在ビルドは成功、
将来Rustでは対応が必要。今回の文書追加と無関係。
Phase 7Kの一過性LockUnavailableは今回未再現、根因解消とはしない。

今回の実RPC通信・秘密鍵/seed読取・ウォレット作成・実署名・TX送信・
Faucet・資産移動・本番保存変更・履歴削除・commit/push・Notion記録は0回。
既存設定の秘密情報を避けたため、現在のalias対応は過去記録以上に確定していない。
Phase 7Mの調査を終了し、次Phaseは開始しない。
