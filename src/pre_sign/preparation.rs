//! Read-only preparation. No signer, broadcast or persistent store capability.
use super::*;
use std::cell::Cell;
#[derive(Debug, PartialEq)]
pub struct Input {
    from: String,
    to: String,
    amount: u64,
    limit: u64,
}
impl Input {
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut fields: [Option<String>; 4] = [None, None, None, None];
        let mut iter = args.iter();
        while let Some(flag) = iter.next() {
            let index = match flag.as_str() {
                "--from" => 0,
                "--to" => 1,
                "--amount-kelvin" => 2,
                "--fee-limit-kelvin" => 3,
                _ => return Err("準備コマンドの未知の引数".into()),
            };
            if fields[index].is_some() {
                return Err("引数重複".into());
            }
            fields[index] = Some(iter.next().ok_or("引数の値が欠損")?.clone());
        }
        let [from, to, amount, limit] = fields;
        let integer = |value: Option<String>| -> Result<u64> {
            let s = value.ok_or("kelvin値が必要")?;
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return Err("kelvinは符号なし整数で指定".into());
            }
            Ok(s.parse()?)
        };
        let input = Self {
            from: from.ok_or("送信元が必要")?,
            to: to.ok_or("送信先が必要")?,
            amount: integer(amount)?,
            limit: integer(limit)?,
        };
        if input.limit == 0 {
            return Err("手数料上限は正のkelvin整数が必要".into());
        }
        input
            .amount
            .checked_add(input.limit)
            .ok_or("送金額と上限のoverflow")?;
        // Reuse the existing address/signer validation before any network operation.
        PreparedTransfer::new_with_amount(
            TESTNET_RPC,
            &input.from,
            &input.to,
            1,
            ConfigHashPrefix::new(0),
            input.amount,
        )?;
        Ok(input)
    }
}
pub struct Report {
    pub balance: Option<u64>,
    pub estimated_fee: Option<u64>,
    pub blocking_reason: Option<String>,
}
impl Report {
    pub fn passed(&self) -> bool {
        self.blocking_reason.is_none()
    }
    pub fn render(&self, input: &Input) -> String {
        let number = |x: Option<u64>| x.map(|v| v.to_string()).unwrap_or_else(|| "UNKNOWN".into());
        format!(
            "Network: Testnet (RPC config照合、独立chain ID未証明)\nRPC: {TESTNET_RPC}\nFrom: {}\nTo: {}\nAmount: {}\nAmount Unit: kelvin\nBalance: {}\nBalance Unit: kelvin\nFee Limit: {}\nFee Limit Unit: kelvin\nEstimated Fee: {} kelvin\nValidation Result: {}\nBlocking Reasons: {}\nSignature Status: NOT EXECUTED\nBroadcast Status: NOT EXECUTED\n準備成功は送金成功ではありません。手数料上限は実徴収額の保証ではありません。",
            input.from,
            input.to,
            input.amount,
            number(self.balance),
            input.limit,
            number(self.estimated_fee),
            if self.passed() {
                "PREPARATION PASSED"
            } else {
                "PREPARATION BLOCKED"
            },
            self.blocking_reason.as_deref().unwrap_or("NONE")
        )
    }
}
struct Observed<'a, R> {
    rpc: &'a R,
    balance: Cell<Option<u64>>,
    fee: Cell<Option<u64>>,
}
impl<R: ReadRpc> ReadRpc for Observed<'_, R> {
    async fn prefix(&self) -> Result<ConfigHashPrefix> {
        self.rpc
            .prefix()
            .await
            .map_err(|_| "config RPC取得失敗".into())
    }
    async fn fee(&self, message: &str) -> Result<Option<u64>> {
        let v = self
            .rpc
            .fee(message)
            .await
            .map_err(|_| "手数料RPC取得失敗")?;
        self.fee.set(v);
        Ok(v)
    }
    async fn balance(&self, from: &Pubkey) -> Result<u64> {
        let v = self
            .rpc
            .balance(from)
            .await
            .map_err(|_| "残高RPC取得失敗")?;
        self.balance.set(Some(v));
        Ok(v)
    }
}
async fn prepare_using(input: &Input, rpc: &impl ReadRpc, clock: &impl Clock) -> Report {
    let observed = Observed {
        rpc,
        balance: Cell::new(None),
        fee: Cell::new(None),
    };
    let outcome = tokio::time::timeout(Duration::from_secs(30), async {
        let prefix = observed.prefix().await?;
        let time = clock.now()?;
        let transfer = PreparedTransfer::new_with_amount(
            TESTNET_RPC,
            &input.from,
            &input.to,
            time.unix_ms,
            prefix,
            input.amount,
        )?;
        let hash = signing_message_hash(transfer.signing_message());
        let checked = check_with_rpc_and_clock(
            transfer,
            hash,
            input.limit,
            &observed,
            DEFAULT_TTL_MS,
            clock,
        )
        .await?;
        let balance = observed.balance.get().ok_or("残高未確認")?;
        if balance
            < input
                .amount
                .checked_add(input.limit)
                .ok_or("金額overflow")?
        {
            return Err("残高不足: 送金額＋手数料上限を満たさない".into());
        }
        checked.validate_current(clock)?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await;
    let reason = match outcome {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e.to_string()),
        Err(_) => Some("準備RPCタイムアウト".into()),
    };
    Report {
        balance: observed.balance.get(),
        estimated_fee: observed.fee.get(),
        blocking_reason: reason,
    }
}
pub async fn prepare(input: &Input) -> Report {
    let rpc = TestnetReadRpc(HttpRpcClient::new(TESTNET_RPC.into()));
    prepare_using(input, &rpc, &SystemClock).await
}
#[cfg(test)]
mod tests {
    use super::*;
    struct ClockMock;
    impl Clock for ClockMock {
        fn now(&self) -> Result<ClockReading> {
            Ok(ClockReading {
                unix_ms: 10000,
                monotonic_ms: 10000,
            })
        }
    }
    struct Mock {
        balance: u64,
        fee: Option<u64>,
        fail: bool,
        change: bool,
        calls: Cell<u32>,
    }
    impl ReadRpc for Mock {
        async fn prefix(&self) -> Result<ConfigHashPrefix> {
            if self.fail {
                return Err("mock fail".into());
            }
            self.calls.set(self.calls.get() + 1);
            Ok(ConfigHashPrefix::new(
                if self.change && self.calls.get() > 1 {
                    43
                } else {
                    42
                },
            ))
        }
        async fn balance(&self, _: &Pubkey) -> Result<u64> {
            Ok(self.balance)
        }
        async fn fee(&self, message: &str) -> Result<Option<u64>> {
            let bytes = STANDARD.decode(message)?;
            let decoded: VersionedMessage = bincode::deserialize(&bytes)?;
            assert_eq!(decoded.serialize_safe()?, bytes);
            let VersionedMessage::Legacy(m) = decoded else {
                panic!("unexpected version")
            };
            assert_eq!(
                u64::from_le_bytes(m.instructions[0].data[4..12].try_into().unwrap()),
                2_000_000
            );
            Ok(self.fee)
        }
    }
    fn args() -> Vec<String> {
        [
            "--from",
            "7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa",
            "--to",
            "FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6",
            "--amount-kelvin",
            "2000000",
            "--fee-limit-kelvin",
            "6000",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    fn mock() -> Mock {
        Mock {
            balance: 2_006_000,
            fee: Some(5000),
            fail: false,
            change: false,
            calls: Cell::new(0),
        }
    }
    #[tokio::test]
    async fn success_and_never_signed_or_broadcast() {
        let input = Input::parse(&args()).unwrap();
        let r = prepare_using(&input, &mock(), &ClockMock).await;
        assert!(r.passed());
        assert_eq!(r.balance, Some(2_006_000));
        assert_eq!(r.estimated_fee, Some(5000));
        let output = r.render(&input);
        assert!(output.contains("PREPARATION PASSED"));
        assert!(output.contains("Signature Status: NOT EXECUTED"));
        assert!(output.contains("Broadcast Status: NOT EXECUTED"));
    }
    #[test]
    fn invalid_inputs() {
        for (index, value) in [
            (1, "bad"),
            (3, "bad"),
            (5, "0"),
            (5, "1.5"),
            (5, "-1"),
            (7, "0"),
            (7, "18446744073709551615"),
        ] {
            let mut a = args();
            a[index] = value.into();
            assert!(Input::parse(&a).is_err());
        }
        let mut a = args();
        a[3] = a[1].clone();
        assert!(Input::parse(&a).is_err());
        let mut a = args();
        a.push("--send".into());
        assert!(Input::parse(&a).is_err());
    }
    #[tokio::test]
    async fn insufficient_balance_for_limit() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.balance = 2_005_000;
        let r = prepare_using(&input, &m, &ClockMock).await;
        assert!(!r.passed());
        assert!(r.blocking_reason.unwrap().contains("手数料上限"));
    }
    #[tokio::test]
    async fn insufficient_balance_for_estimate() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.balance = 2_004_999;
        assert!(!prepare_using(&input, &m, &ClockMock).await.passed());
    }
    #[tokio::test]
    async fn fee_limit_exceeded() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.fee = Some(6001);
        assert!(!prepare_using(&input, &m, &ClockMock).await.passed());
    }
    #[tokio::test]
    async fn missing_fee_blocks() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.fee = None;
        let r = prepare_using(&input, &m, &ClockMock).await;
        assert!(!r.passed());
        assert_eq!(r.estimated_fee, None);
    }
    #[tokio::test]
    async fn rpc_failure_blocks() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.fail = true;
        assert!(!prepare_using(&input, &m, &ClockMock).await.passed());
    }
    #[tokio::test]
    async fn config_mismatch_blocks() {
        let input = Input::parse(&args()).unwrap();
        let mut m = mock();
        m.change = true;
        assert!(!prepare_using(&input, &m, &ClockMock).await.passed());
    }
}
