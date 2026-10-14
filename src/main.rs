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
    Prepare(Pubkey, Pubkey),
    Help,
}
fn parse(args: &[String]) -> Result<Mode> {
    if args == ["--help"] {
        return Ok(Mode::Help);
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
async fn run(mode: Mode) -> Result<()> {
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
            eprintln!("安全停止: {e}");
            std::process::exit(1);
        }
    };
    if mode == Mode::Help {
        println!(
            "準備専用: rialo-first --from PUBLIC_KEY --to PUBLIC_KEY\nRPC確認: rialo-first --rpc-check\n金額固定0.001 RLO。鍵読取・署名・送信なし。"
        );
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
