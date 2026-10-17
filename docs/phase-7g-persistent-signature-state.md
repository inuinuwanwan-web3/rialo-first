# Phase 7G — 署名要求状態の永続化

## 調査と再利用

Phase 7F の SignatureLifecycle::new はプロセス内 AtomicU64 から要求IDを生成する
（src/pre_sign/lifecycle.rs:55）。このIDだけでは再起動後の一意性を保証しない。
同ファイルの validate_using、begin_signing、accept_response をラップし、
Phase 7E の再検証・期限管理と Phase 7B の PreparedTransfer::assemble
（src/lib.rs:83）を再利用する。署名検証の別実装は追加しない。

## 永続化方式

Unix のローカルファイルシステム向け StateStore を追加。
新規作成は initialize、既存ストアは open を明示的に使い分ける。
既存ディレクトリの上書き、欠損ストアの自動作成、破損時の初期化は行わない。

requests.journal に追記専用JSONレコードを保存し、各書き込み後に sync_all。
保存する項目は request_id、protocol_version、signing_message_hash、
network_config_prefix、created_at、expires_at、state、updated_at。
署名・署名対象生バイト列・秘密鍵・seed・認証情報は保存しない。
SHA-256チェックサムを前レコードに連結し、構造・状態遷移も読み込み時に検査する。
これは破損検出であり、悪意ある書き換えに対する認証ではない。

ディレクトリは0700、ファイルは0600で新規作成する。
既存パスのシンボリックリンク、不適切な型、group/otherアクセス権を拒否。
File::try_lock の排他ロックをストアの生存期間全体で保持する。
同一プロセス内は &mut 借用、別ハンドル・別プロセスはOSロックで競合を拒否する。

一時ファイル＋rename方式は、置換によりロック対象inodeと保存先が分離する
可能性を避けるため採用しなかった。単一inodeへの追記とfsyncを採用。
初回はファイル・ディレクトリ・親ディレクトリも同期する。
上限16MiB、1レコード4096バイト。上限到達時は停止し、自動削除・圧縮しない。

serde は既にロック済みの1.0.229を直接依存として追加し、
既存serde_jsonを開発依存から通常依存へ移動。新しいパッケージの取得は不要。
標準ライブラリのFileロックAPIを使用（今回のRust 1.98環境で検証）。

## 状態遷移と使い方

StateStore::register(id, checked, clock) で PersistentLifecycle を得る。
呼び出し側がIDを明示し、履歴に同じIDがあれば内容・状態に関係なく拒否する。
ID衝突時に自動で代替IDを生成しない。

正常経路：
CREATED → VALIDATING → VALIDATED → AWAITING_SIGNATURE
→ PROCESSING_RESPONSE → SIGNED

VALIDATINGとPROCESSING_RESPONSEは永続化層の処理途中状態。
検証失敗・期限切れはREJECTED / EXPIREDに記録する。
保存に失敗したセッションは停止し、I/O失敗後のストアは再利用できない。
時計などの異常で終端状態自体を保存できない場合、以前の状態が残るが
要求IDは登録済みなので再起動後の再利用も拒否される。

validate は既存の最新RPC再検証を利用するため、本番APIを呼ぶと読み取りRPCが発生する。
今回のテストは内部モックのみ使用し、実RPCは呼び出していない。
begin_signing は要求を返す前にAWAITING_SIGNATUREを保存。
accept_response は検証前にPROCESSING_RESPONSEを保存し、
既存署名検証・TX構築後にSIGNEDの同期保存が成功した場合だけwire bytesを返す。
二回目の応答は拒否する。署名者との通信やTX送信は実装していない。

## 再起動・クラッシュ時の方針と制限

再起動後は履歴の全要求IDを使用済みとして扱う。
SIGNED / REJECTED / EXPIREDに加え、処理途中・期限切れの要求も再開しない。
時計の単調時刻やCheckedTransferをディスクから復元するAPIは提供しない。
再登録時に同じIDを異なるメッセージへ流用することも拒否する。

保存済みPROCESSING_RESPONSEの後にクラッシュした場合、再試行を禁止する。
SIGNED保存後、呼び出し側がTXを受け取る前にクラッシュした場合も再取得不可。
可用性より二重処理防止を優先し、出力のexactly-once配達は保証しない。
部分書き込み・不正JSON・チェーン不一致等はopenを失敗させ、自動修復しない。

保証の前提は、信頼できる保存先と、全利用者が同じストア・ロックを使うこと。
従来のインメモリPhase 7F APIは互換性のため残っており、それ単独では永続化されない。
別ストアの作成、ストアの削除・バックアップ巻き戻し、正常なレコード境界での
末尾切り詰め、同一ユーザーによる悪意あるパス差し替えは防げない。
チェックサムは外部の改ざん防止アンカーではない。
NFS等の共有FS、実電源断・ストレージ障害時のfsync保証は未検証。
異常終了後の運用復旧・ログ容量管理・Windows対応は未実装。
この範囲を超える耐障害性や敵対的改ざん耐性を完成済みとは扱わない。

## オフライン検証

既存66件＋新規14件＝80件成功（library 73、binary 7）。
新規テストは一時ディレクトリとメモリ内の合成Ed25519鍵だけを使用。

- 正常保存・再読み込み・SIGNED後の二重応答と再登録拒否
- REJECTED / EXPIREDの再起動後再利用拒否
- 同一ID・異なるメッセージへのID流用拒否
- 放棄された要求・期限切れ要求の再開拒否
- 登録、要求公開前、応答処理前の実I/O保存失敗
- SIGNED保存失敗の注入によるTX出力抑止と再起動後拒否
- 同一プロセス内の競合と別プロセスの排他ロック
- 欠損・破損・チェックサム不一致・不適切な権限の拒否

cargo fmt --check、cargo check --offline --locked、
cargo test --offline --locked、git diff --check が成功。
依存proc-macro-error2の将来互換性警告は既存のまま残る。

実ウォレットアクセス・実署名・TX送信・Faucet・commit・pushはすべて0回。
次の作業は、実ウォレットを使わず強制終了を注入するクラッシュ試験と
復旧手順の検証。自動的な履歴リセットは導入しない。
