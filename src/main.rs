use base64::{Engine as _, engine::general_purpose::STANDARD};
use rialo_cdk::{
    RpcClient,
    constants::URL_DEVNET,
    rpc::{HttpRpcClient, types::Pubkey},
};
use rialo_s_message::{ConfigHashPrefix, VersionedMessage};
use std::{
    error::Error,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const AMOUNT: u64 = rialo_first::TRANSFER_AMOUNT;
#[derive(Debug, PartialEq)]
enum Mode {
    RpcCheck,
    TestnetPrepare(rialo_first::pre_sign::preparation::Input),
    TestnetReadOnly(Option<Pubkey>),
    Prepare(Pubkey, Pubkey),
    Help,
}
fn parse(args: &[String]) -> Result<Mode> {
    if args.first().map(String::as_str) == Some("--testnet-prepare-transfer") {
        return Ok(Mode::TestnetPrepare(
            rialo_first::pre_sign::preparation::Input::parse(&args[1..])?,
        ));
    }
    if args == ["--help"] {
        return Ok(Mode::Help);
    }
    if args.first().map(String::as_str) == Some("--testnet-readonly") {
        return match args.len() {
            1 => Ok(Mode::TestnetReadOnly(None)),
            3 if args[1] == "--address" => Ok(Mode::TestnetReadOnly(Some(
                Pubkey::from_str(&args[2]).map_err(|_| "公開アドレスの形式が不正")?,
            ))),
            _ => Err("Testnet読み取り専用: --testnet-readonly [--address PUBLIC_KEY]".into()),
        };
    }
    if args == ["--rpc-check"] {
        return Ok(Mode::RpcCheck);
    }
    let (mut from, mut to) = (None, None);
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let slot = match flag.as_str() {
            "--from" => &mut from,
            "--to" => &mut to,
            _ => return Err("未知の引数。--helpを参照してください".into()),
        };
        if slot.is_some() {
            return Err("アドレス引数の重複".into());
        }
        let value = iter.next().ok_or("アドレス未指定")?;
        *slot = Some(Pubkey::from_str(value).map_err(|_| "公開アドレスの形式が不正")?);
    }
    let from = from.ok_or("--fromが必要。接続確認のみなら--rpc-checkを指定")?;
    let to = to.ok_or("--toが必要。送金先は自動決定しません")?;
    if from == to {
        return Err("この準備ツールでは送付元と宛先を別アドレスにしてください".into());
    }
    Ok(Mode::Prepare(from, to))
}
fn rlo(n: u64) -> String {
    format!("{}.{:09}", n / 1_000_000_000, n % 1_000_000_000)
}
fn remaining(balance: u64, fee: Option<u64>) -> Result<(u64, u64)> {
    let fee = fee.ok_or("手数料未取得")?;
    let required = AMOUNT.checked_add(fee).ok_or("金額オーバーフロー")?;
    Ok((fee, balance.checked_sub(required).ok_or("残高不足")?))
}
// API 0.18.1 get_fee_for_message.rs:16: base64 bincode VersionedMessage.
// versions/mod.rs:198: Legacy has no version prefix. Verify canonical bytes.
fn message(from: Pubkey, to: Pubkey, time: i64, hash: ConfigHashPrefix) -> Result<String> {
    let builder = rialo_first::transfer_builder(from, to, time, hash);
    let bytes = builder.build()?; // Message only: no signatures or transaction envelope.
    let decoded: VersionedMessage = bincode::deserialize(&bytes)?;
    let canonical = decoded.serialize_safe()?;
    if canonical != bytes {
        return Err("メッセージ形式不一致".into());
    }
    Ok(STANDARD.encode(canonical))
}
async fn testnet_readonly(address: Option<Pubkey>) -> Result<()> {
    let endpoint = rialo_first::TESTNET_RPC;
    println!("Read-only RPC: {endpoint}");
    let client = HttpRpcClient::new(endpoint.to_owned());
    let prefix = client
        .get_config_hash_prefix()
        .await
        .map_err(|_| "Testnet config取得失敗")?;
    println!("Config hash prefix: {prefix:?} (chain identity未証明)");
    let height = client
        .get_block_height()
        .await
        .map_err(|_| "Testnet block height取得失敗")?;
    println!("Block height: {height}");
    if let Some(address) = address {
        let balance = client
            .get_balance(&address)
            .await
            .map_err(|_| "Testnet balance取得失敗")?;
        println!(
            "Public address: {address}\nBalance: {balance} kelvin ({} RLO)",
            rlo(balance)
        );
    }
    println!("読み取り完了。署名・送信・状態保存なし。");
    Ok(())
}
async fn run(mode: Mode) -> Result<()> {
    if let Mode::TestnetPrepare(input) = mode {
        let report = rialo_first::pre_sign::preparation::prepare(&input).await;
        println!("{}", report.render(&input));
        return if report.passed() {
            Ok(())
        } else {
            Err("送金準備の必須条件を満たしていません".into())
        };
    }
    if let Mode::TestnetReadOnly(address) = mode {
        return testnet_readonly(address).await;
    }
    let client = HttpRpcClient::new(URL_DEVNET.to_string());
    let height = client.get_block_height().await?;
    let hash = client.get_config_hash_prefix().await?;
    println!(
        "Rialo DevNet connected!\nRPC: {URL_DEVNET}\nBlock height: {height}\nConfig hash: {hash:?}"
    );
    let Mode::Prepare(from, to) = mode else {
        return Ok(());
    };
    let balance = client.get_balance(&from).await?;
    let valid_from = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let encoded = message(from, to, valid_from, hash)?;
    let quote = client.get_fee_for_message(&encoded).await?;
    println!(
        "送付元: {from}\n送金先: {to}\n残高: {} RLO\n送金額: 0.001000000 RLO",
        rlo(balance)
    );
    let (fee, after) = remaining(balance, quote.value)?;
    let end_height = client.get_block_height().await?;
    println!(
        "手数料: {} RLO ({fee} kelvin)\n計算上の残高: {} RLO\n照会期間のブロック高: {height}..{end_height}\n有効時刻(ms): {valid_from}",
        rlo(fee),
        rlo(after)
    );
    println!(
        "準備照会完了。各照会は同一ブロックのスナップショットではありません。\n署名者との一致: 未確認\n宛先の所有者・受取条件: 未確認\n署名・送信機能なし。この見積りは実行承認ではありません。"
    );
    Ok(())
}
#[tokio::main]
async fn main() {
    let mode = match parse(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(mode) => mode,
        Err(e) => {
            if std::env::args().nth(1).as_deref() == Some("--testnet-prepare-transfer") {
                println!(
                    "Network: Testnet\nRPC: {}\nFrom: UNKNOWN\nTo: UNKNOWN\nAmount: UNKNOWN\nAmount Unit: kelvin\nBalance: UNKNOWN\nBalance Unit: kelvin\nFee Limit: UNKNOWN\nFee Limit Unit: kelvin\nEstimated Fee: UNKNOWN\nValidation Result: PREPARATION BLOCKED\nBlocking Reasons: 入力検証失敗\nSignature Status: NOT EXECUTED\nBroadcast Status: NOT EXECUTED",
                    rialo_first::TESTNET_RPC
                );
            }
            eprintln!("安全停止: {e}");
            std::process::exit(1);
        }
    };
    if mode == Mode::Help {
        println!(
            "Testnet準備: --testnet-prepare-transfer --from PUBLIC_KEY --to PUBLIC_KEY --amount-kelvin INTEGER --fee-limit-kelvin INTEGER\nTestnet読取: rialo-first --testnet-readonly [--address PUBLIC_KEY]\n準備専用: rialo-first --from PUBLIC_KEY --to PUBLIC_KEY\nRPC確認: rialo-first --rpc-check\n従来DevNet準備は金額固定0.001 RLO。Testnet準備はkelvin整数指定。鍵読取・署名・送信なし。"
        );
        return;
    }
    if matches!(mode, Mode::TestnetPrepare(_)) {
        if let Err(e) = run(mode).await {
            eprintln!("安全停止: {e}");
            std::process::exit(1);
        }
        return;
    }
    match tokio::time::timeout(Duration::from_secs(30), run(mode)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("安全停止: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("安全停止: RPCタイムアウト");
            std::process::exit(1);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn ordinary_invocation_is_prepare_only() {
        let from = Pubkey::from([1; 32]);
        let to = Pubkey::from([2; 32]);
        assert_eq!(
            parse(&args(&[
                "--from",
                &from.to_string(),
                "--to",
                &to.to_string()
            ]))
            .unwrap(),
            Mode::Prepare(from, to)
        );
    }
    #[test]
    fn send_flags_fail_before_rpc_or_key_access() {
        let from = Pubkey::from([1; 32]).to_string();
        let to = Pubkey::from([2; 32]).to_string();
        // Even both flags must stay disabled until CLI/CDK key compatibility is proven.
        for flags in [
            vec!["--send"],
            vec!["--confirm-send"],
            vec!["--send", "--confirm-send"],
        ] {
            let mut input = args(&["--from", &from, "--to", &to]);
            input.extend(args(&flags));
            assert!(parse(&input).is_err());
        }
    }
    #[test]
    fn amount_override_is_rejected() {
        let from = Pubkey::from([1; 32]).to_string();
        let to = Pubkey::from([2; 32]).to_string();
        // Amount is still fixed, not a newly implemented configurable amount.
        assert!(parse(&args(&["--from", &from, "--to", &to, "--amount", "0"])).is_err());
    }
    #[test]
    fn network_override_cannot_enable_sending() {
        let from = Pubkey::from([1; 32]).to_string();
        let to = Pubkey::from([2; 32]).to_string();
        for network in ["devnet", "mainnet", "testnet"] {
            assert!(
                parse(&args(&[
                    "--from",
                    &from,
                    "--to",
                    &to,
                    "--network",
                    network,
                    "--send",
                    "--confirm-send"
                ]))
                .is_err()
            );
        }
    }
    #[test]
    fn inputs_fail_closed() {
        for a in [
            vec![],
            vec!["--from"],
            vec!["--from", "invalid", "--to", "invalid"],
            vec!["--send"],
            vec!["--rpc-check", "--to", "invalid"],
        ] {
            assert!(parse(&args(&a)).is_err());
        }
        let a = Pubkey::from([1; 32]).to_string();
        let b = Pubkey::from([2; 32]).to_string();
        assert!(parse(&args(&["--from", &a, "--to", &b])).is_ok());
        assert!(parse(&args(&["--from", &a, "--to", &a])).is_err());
        assert!(parse(&args(&["--from", &a, "--from", &a, "--to", &b])).is_err());
    }
    #[test]
    fn balance_boundaries() {
        assert!(remaining(u64::MAX, None).is_err());
        assert!(remaining(u64::MAX, Some(u64::MAX)).is_err());
        assert!(remaining(AMOUNT + 4, Some(5)).is_err());
        assert_eq!(remaining(AMOUNT + 5, Some(5)).unwrap(), (5, 0));
        assert_eq!(rlo(894559480), "0.894559480");
    }
    #[test]
    fn canonical_unsigned_transfer() {
        // Synthetic public keys used only offline; no wallet or keypair creation.
        let from = Pubkey::from([1; 32]);
        let to = Pubkey::from([2; 32]);
        let encoded = message(from, to, 1234, ConfigHashPrefix::new(42)).unwrap();
        let decoded: VersionedMessage =
            bincode::deserialize(&STANDARD.decode(encoded).unwrap()).unwrap();
        let VersionedMessage::Legacy(m) = decoded else {
            panic!("expected legacy");
        };
        assert_eq!(m.header.num_required_signatures, 1);
        assert_eq!(m.account_keys[0], from);
        assert_eq!(m.valid_from, 1234);
        assert_eq!(m.instructions.len(), 1);
        let ix = &m.instructions[0];
        assert_eq!(m.account_keys[ix.accounts[0] as usize], from);
        assert_eq!(m.account_keys[ix.accounts[1] as usize], to);
        assert_eq!(
            m.account_keys[ix.program_id_index as usize].to_string(),
            "11111111111111111111111111111111"
        );
        let mut expected = 2u32.to_le_bytes().to_vec();
        expected.extend_from_slice(&AMOUNT.to_le_bytes());
        assert_eq!(ix.data, expected);
    }
}

#[cfg(test)]
mod testnet_tests {
    use super::*;
    #[test]
    fn explicit_testnet_readonly_mode() {
        assert_eq!(
            parse(&["--testnet-readonly".into()]).unwrap(),
            Mode::TestnetReadOnly(None)
        );
        let address = Pubkey::from([2; 32]);
        assert_eq!(
            parse(&[
                "--testnet-readonly".into(),
                "--address".into(),
                address.to_string()
            ])
            .unwrap(),
            Mode::TestnetReadOnly(Some(address))
        );
    }
    #[test]
    fn testnet_rejects_mutation_flags_and_invalid_address() {
        for tail in [
            vec!["--send"],
            vec!["--confirm-send"],
            vec!["--address", "invalid"],
            vec!["--rpc", "https://other.invalid"],
        ] {
            let mut args = vec!["--testnet-readonly".to_owned()];
            args.extend(tail.into_iter().map(str::to_owned));
            assert!(parse(&args).is_err());
        }
    }
}

#[cfg(test)]
mod preparation_cli_tests {
    use super::*;
    #[test]
    fn preparation_mode_requires_all_explicit_fields() {
        let args = [
            "--testnet-prepare-transfer",
            "--from",
            "7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa",
            "--to",
            "FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6",
            "--amount-kelvin",
            "2000000",
            "--fee-limit-kelvin",
            "6000",
        ]
        .map(str::to_owned);
        assert!(matches!(parse(&args).unwrap(), Mode::TestnetPrepare(_)));
        assert!(parse(&args[..7]).is_err());
        let mut duplicate = args.to_vec();
        duplicate.extend(["--from".into(), "unused".into()]);
        assert!(parse(&duplicate).is_err());
    }
}
