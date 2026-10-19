# Phase 7P — Transaction Construction & CLI/CDK Compatibility

## 判定

未署名構築・オフライン検証は完了。CLI完全互換性と手数料0の原因は
未検証のため、Phase 7Pの全完了条件は未達成として停止する。
既存構築機能が十分なので本番機能や独自TX形式を追加していない。

調査対象：Cargo.toml/lock（CDK 0.18.1）、main、pre_sign、preparation、
PreparedTransfer、7M〜7O、過去CLI送金記録、ローカルCDKソース。
AGENTS.mdは対象/祖先に見つからなかった。開始時は調査文書だけ未追跡。
docs/phase-7-signing-investigation.mdを変更していない。

## CLI/CDK比較

CDK根拠の基点：
/home/pc/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/
以下のCDKはrialo-cdk-0.18.1/srcを指す。

| 項目 | CDKで確認できた仕様 | CLIとの対応・未検証 |
| --- | --- | --- |
| TX形式 | transaction/builder.rs:529 build_transaction、:29ゼロ署名枠付きTransaction | CLI生wire記録なし、完全一致は未検証 |
| From/To | Pubkey 32バイト、入力はBase58。アカウント表と命令index | 過去公開アドレスは一致。CLI内部実装未確認 |
| 金額 | transfer命令u32識別子2＋u64 kelvin、little endian | CLI helpはRLO表記。過去0.001 RLO=1,000,000 kelvin |
| 手数料単位 | constants.rs:54、1 RLO=10^9 kelvin | 過去CLI実績5,000 kelvin |
| 手数料計算 | getFeeForMessageで見積もる。現在サーバー料金式は未確認 | CLIの内部照会/指定方式未確認 |
| Network/Chain | message内config_hash_prefix、接続URLは外部設定 | prefixは不変chain IDでない。過去と現在でprefixが異なる |
| Nonce等 | builder.rs:660 valid_from i64 Unixミリ秒とconfigをmessageへ格納。専用nonce欄はこのtransfer構造にない | 同時刻・同一入力は同じmessage。再実行防止保証やCLI内部方針は未確認 |
| 有効期限 | アプリpre_sign.rs:36/53はチェックTTL。messageにはvalid_fromがある | アプリTTLとネットワークTX寿命は別。ネットワーク受理窓は未確認 |
| 署名対象 | builder.rs:488はbuildの正確なmessage bytesを署名 | CLIの同一バイト列は未確認 |
| 署名方式 | keyring/traits.rs:24/110 Ed25519 | CLI鍵/aliasの正式互換性は未確認 |
| シリアライズ | builder.rs:645 message、:705 compact署名配列＋message | 名前だけSolana互換と見なさない。独自valid_from/config/OCCを持つ |
| TX ID | rpc/client.rs:524が返却signature文字列をSignatureへ変換 | 未署名messageのSHA-256をTX IDとしない。CLI/稼働ノードの計算規約は未確認 |

rialo-s-transaction-0.18.1/src/sanitized.rs:162は先頭signatureを返すが、
それだけで稼働RPC全体のTX ID規約を証明したとはしない。
未署名ゼロ署名枠を実TX IDとして扱わない。

CLI本体ソースは前Phaseで調査したインストール先/registryに見つからず、
今回はrialo client transfer --helpで
<TO> <AMOUNT> [FROM] [PAYER]を再確認しただけ。
CLIを使った署名やネットワーク比較はしていない。
既存docs/2026-10-11-testnet-transfer-e2e.md:72以降の実績はCLI経路のみ。
raw TXやCLI構築時刻等が残っていないため、その取引のbyte-for-byte再構築は不可。

## 正式API・未署名構築

src/lib.rs:59 PreparedTransfer::new_with_amountは、
共有Builderヘルパーでadd_transfer_instructionを呼び、
CDK build_transactionでTransactionを作成する。
既存機能を再利用し、新しい公開署名/送信経路を追加しない。

CDK Transaction::newは必要署名数だけ64バイトゼロ枠を置く。
serialize_wiredはゼロ枠でもwireを生成できる。
これは構造上の未署名エンベロープであり、有効な署名済み取引ではない。
is_fully_signedだけも暗号学的検証ではないため、送信可否に代用しない。

追加テストsrc/unsigned_tests.rs:18はPreparedTransfer内部のCDK Transactionを
そのままserialize_wired/deserialize_wiredし、以下を確認する：
必要署名数1、全ゼロ署名、from/to、System Program、金額1 kelvin、
明示的mock valid_from=10000/config=42、canonical messageとwireの往復一致。
これらの時刻/configはテスト専用で、実Testnet情報を補完したものではない。
production APIでは時刻/configが型として必須だが、その値だけでは新鮮さや
ネットワーク一致は保証しない。実行前のpre_sign_checkが別途必要。

## 手数料0 kelvinの調査

正式API：
rialo-api-types-0.18.1/src/messages/get_fee_for_message.rs:16は
「bincode serialized VersionedMessageのbase64」を要求する。
CDK rpc/client.rs:766はその文字列をRPC requestに入れる。
アプリpre_sign.rs:274はPreparedTransfer.signing_messageをbase64化する。
署名配列を含む完全なwireを照会へ渡す処理ではない。

追加テスト:73は
- messageのbase64 decode → VersionedMessage deserialize → serialize_safe一致
- 完全wireとは異なること
- 未署名でもheaderのrequired_signaturesは1であること
を確認した。「未署名だから必要署名数0になった」とする説明は今回の
構築コードと一致しない。署名を作って検証したわけではない。

ローカルrialo-s-fee-structure-0.18.1/src/lib.rs:151/159には
kelvins_per_signature引数が0ならFeeDetails::default()を返す経路がある。
ただしこの関数はdeprecatedで、現Testnetがこのコード/設定を使う証拠はない。
従って「サーバー料金設定が0だった」は可能性であり結論ではない。

7Oの0は当時のRPC観測値。raw messageとraw responseを完全保存しておらず、
当時の時刻/configを含むリクエストを厳密に再現したものではない。
現在サーバーの設定・実装も今回外部照会していない。
過去CLI実績5,000との差の理由、将来の実徴収額は未解決。

手数料上限6000はアプリの見積もり/残高判定用。
messageにはその上限を強制する欄を追加していない。
見積もり0を無料保証や実手数料上限保証とは扱わない。

## テスト・変更ファイル

- src/lib.rs末尾：cfg(test) mod unsigned_testsを追加。本番処理の変更なし。
- src/unsigned_tests.rs:18：未署名各フィールド・wire往復
- 同:48：同じ入力の再現性、金額・時刻・prefix・宛先変更の反映
- 同:73：見積もりmessageとTX envelopeの区別
- 同:89：不正/不足wireと不正署名枠長の拒否
- 同:99：欠損/無効アドレス、同一宛先、amount0、別endpoint拒否
- 本文書

新規5件に秘密鍵生成・署名・RPCクライアント・ストア操作はない。
不正wireの全パターンを網羅した保証ではない。
overflow、fee未取得/上限超過、prefix不一致、時計/期限切れは既存
preparation/pre_sign/lifecycleテストを再利用し、件数目的で重複追加しない。
検証結果の期限切れと、ネットワークTX有効期限の検証は別である。
整数型に表現できない金額・欠損フラグは既存Input::parseが拒否する。
負のRPC fee等は既存APIのu64型で拒否される。
ノードのfee計算、nonce再利用、実TX受理期限の試験は仕様不足につき未実施。

cargo fmt --check、cargo check --offline --locked、
cargo test --offline --locked、git diff --check：成功。
既存129件＋新規5件＝134件成功、失敗0（library124、binary10）。
通常の並列実行でLockUnavailableは今回再発しなかった。
既存129件に含まれるメモリ内合成署名テストは再実行したが、
実ウォレット署名は0回。新規5件は署名処理自体を呼ばない。
既存proc-macro-error2の将来互換性警告1種類が残る。

## 完了条件と残作業

| 条件 | 判定 |
| --- | --- |
| A 正式TX形式 | 検証済み：ローカルCDK 0.18.1の形式 |
| B 未署名TX構築 | 検証済み：既存PreparedTransferと正式API |
| C CLI互換性 | 未検証：CLIソース/raw構築fixture不足 |
| D 手数料処理説明 | 検証済み：照会payloadと上限判定。0の根因は未検証 |
| E オフライン試験 | 検証済み：134件成功 |
| F 実署名・送信なし | 検証済み：実ウォレット操作・通信なし |

実署名までには正式なCLI/外部署名者接続仕様、公開鍵の照合、
ネットワーク有効期間、最新残高・見積もり、ユーザー承認が必要。
最小の次作業は、対象CLIバージョンの構築/署名コードまたは公式な
未署名fixtureと手数料RPCのサーバー仕様を入手し、今回の確認済みmessageと照合すること。
その資料なしに独自署名形式・nonce・TX IDを作らない。
今回は資料不足の境界で停止し、次Phaseを開始しない。

実ウォレットアクセス、秘密鍵/seed読取、実署名、TX送信、Faucet、
外部通信、署名状態変更、commit/push、Notion記録は0回。
