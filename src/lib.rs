//! External-signature assembly and opt-in read-only pre-sign RPC checks. No wallet access.
pub mod pre_sign;
use ed25519_dalek::{Signature, VerifyingKey};
use rialo_cdk::{TransactionBuilder, rpc::types::Pubkey, transaction::builder::Transaction};
use rialo_s_message::{ConfigHashPrefix, VersionedMessage};
use std::{error::Error, str::FromStr};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const TESTNET_RPC: &str = "https://testnet.rialo.io/";
pub const TRANSFER_AMOUNT: u64 = 1_000_000;

/// Fixed single-signer transfer, shared with the read-only preparation executable.
pub fn transfer_builder(
    from: Pubkey,
    to: Pubkey,
    valid_from: i64,
    hash: ConfigHashPrefix,
) -> TransactionBuilder {
    let mut builder = TransactionBuilder::new(from, valid_from, hash);
    builder.add_transfer_instruction(&from, &to, TRANSFER_AMOUNT);
    builder
}

/// Detached Ed25519 signature: raw 64 bytes, not Base64/Base58 or a signed envelope.
pub struct ExternalSignature<'a> {
    pub public_address: &'a str,
    pub signature: &'a [u8],
}

/// Immutable message prepared locally, never an arbitrary caller-supplied transaction.
/// The endpoint is an offline policy check, not proof of the supplied hash's network.
pub struct PreparedTransfer {
    transaction: Transaction,
    from: Pubkey,
}

impl PreparedTransfer {
    pub fn new(
        rpc: &str,
        from: &str,
        to: &str,
        valid_from: i64,
        hash: ConfigHashPrefix,
    ) -> Result<Self> {
        if rpc != TESTNET_RPC {
            return Err("外部署名の組み込みはTestnetのみ対応".into());
        }
        let from = Pubkey::from_str(from).map_err(|_| "送金元アドレスが不正")?;
        let to = Pubkey::from_str(to).map_err(|_| "送金先アドレスが不正")?;
        if from == to {
            return Err("送金元と送金先が同一".into());
        }
        // Reject invalid/weak signer public keys before requesting any signature.
        let key = VerifyingKey::from_bytes(&from.to_bytes())?;
        if key.is_weak() {
            return Err("送金元の署名公開鍵が不正".into());
        }
        let transaction = transfer_builder(from, to, valid_from, hash).build_transaction()?;
        let decoded: VersionedMessage = bincode::deserialize(transaction.message())?;
        if decoded.serialize_safe()? != transaction.message() {
            return Err("メッセージ形式不一致".into());
        }
        let VersionedMessage::Legacy(message) = decoded else {
            return Err("未対応のメッセージ形式".into());
        };
        if message.header.num_required_signatures != 1
            || message.account_keys.first() != Some(&from)
            || transaction.signature_count() != 1
        {
            return Err("送金元と必要署名者が不一致".into());
        }
        Ok(Self { transaction, from })
    }

    /// Sign exactly these raw bytes with Ed25519 (no envelope, encoding or prehash).
    /// Includes sender, recipient, fixed amount, valid_from and config hash prefix.
    pub fn signing_message(&self) -> &[u8] {
        self.transaction.message()
    }

    /// Verify all detached inputs before assembling wire bytes; never sends them.
    /// Consuming self prevents signature overwrites on a previously assembled object.
    pub fn assemble(mut self, signatures: &[ExternalSignature<'_>]) -> Result<Vec<u8>> {
        if signatures.is_empty() {
            return Err("署名不足".into());
        }
        if signatures.len() != 1 {
            return Err("重複または余分な署名".into());
        }
        let input = &signatures[0];
        let signer = Pubkey::from_str(input.public_address).map_err(|_| "署名者アドレスが不正")?;
        if signer != self.from {
            return Err("送金元と署名者公開鍵が不一致".into());
        }
        let signature = Signature::from_slice(input.signature)?;
        VerifyingKey::from_bytes(&signer.to_bytes())?
            .verify_strict(self.signing_message(), &signature)
            .map_err(|_| "署名検証失敗")?;
        self.transaction
            .add_signature_for_pubkey(&signer, signature.to_bytes().to_vec())?;
        self.transaction.serialize_wired().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    // Synthetic test-only key, instantiated in memory; never printed or persisted.
    fn fixture() -> (SigningKey, String, String) {
        let key = SigningKey::from_bytes(&[42; 32]);
        let from = Pubkey::from(key.verifying_key().to_bytes()).to_string();
        (key, from, Pubkey::from([2; 32]).to_string())
    }
    fn prepare(from: &str, to: &str) -> PreparedTransfer {
        PreparedTransfer::new(TESTNET_RPC, from, to, 1234, ConfigHashPrefix::new(42)).unwrap()
    }
    #[test]
    fn valid_signature_round_trips_exact_message() {
        let (key, from, to) = fixture();
        let prepared = prepare(&from, &to);
        let message = prepared.signing_message().to_vec();
        let signature = key.sign(&message).to_bytes();
        let wire = prepared
            .assemble(&[ExternalSignature {
                public_address: &from,
                signature: &signature,
            }])
            .unwrap();
        // One compact signature count byte, exactly 64 signature bytes, raw message.
        assert_eq!(wire[0], 1);
        assert_eq!(&wire[1..65], &signature);
        assert_eq!(&wire[65..], &message);
        let tx = Transaction::deserialize_wired(&wire).unwrap();
        assert_eq!(tx.message(), message);
        key.verifying_key()
            .verify_strict(
                tx.message(),
                &Signature::from_slice(&tx.signatures[0]).unwrap(),
            )
            .unwrap();
    }
    #[test]
    fn rejects_invalid_and_malformed_signatures() {
        let (_, from, to) = fixture();
        for signature in [vec![0; 64], vec![1; 63], vec![1; 65]] {
            assert!(
                prepare(&from, &to)
                    .assemble(&[ExternalSignature {
                        public_address: &from,
                        signature: &signature
                    }])
                    .is_err()
            );
        }
    }
    #[test]
    fn rejects_wrong_public_key_and_wrong_signing_key() {
        let (key, from, to) = fixture();
        let signature = key.sign(prepare(&from, &to).signing_message()).to_bytes();
        assert!(
            prepare(&from, &to)
                .assemble(&[ExternalSignature {
                    public_address: &to,
                    signature: &signature
                }])
                .is_err()
        );
        let wrong = SigningKey::from_bytes(&[43; 32]);
        let signature = wrong.sign(prepare(&from, &to).signing_message()).to_bytes();
        assert!(
            prepare(&from, &to)
                .assemble(&[ExternalSignature {
                    public_address: &from,
                    signature: &signature
                }])
                .is_err()
        );
    }
    #[test]
    fn rejects_missing_and_duplicate_signatures() {
        let (key, from, to) = fixture();
        assert!(prepare(&from, &to).assemble(&[]).is_err());
        let signature = key.sign(prepare(&from, &to).signing_message()).to_bytes();
        assert!(
            prepare(&from, &to)
                .assemble(&[
                    ExternalSignature {
                        public_address: &from,
                        signature: &signature
                    },
                    ExternalSignature {
                        public_address: &from,
                        signature: &signature
                    },
                ])
                .is_err()
        );
    }
    #[test]
    fn rejects_invalid_addresses_and_same_recipient() {
        let (_, from, to) = fixture();
        for (sender, recipient) in [
            ("invalid", to.as_str()),
            (from.as_str(), "invalid"),
            (from.as_str(), from.as_str()),
        ] {
            assert!(
                PreparedTransfer::new(
                    TESTNET_RPC,
                    sender,
                    recipient,
                    1234,
                    ConfigHashPrefix::new(42)
                )
                .is_err()
            );
        }
        assert!(
            prepare(&from, &to)
                .assemble(&[ExternalSignature {
                    public_address: "invalid",
                    signature: &[0; 64]
                }])
                .is_err()
        );
    }
    #[test]
    fn rejects_non_testnet_endpoints() {
        let (_, from, to) = fixture();
        for rpc in [
            "http://devnet.rialo.io:4100",
            "https://mainnet.rialo.io/",
            "http://testnet.rialo.io/",
            "https://testnet.rialo.io.evil/",
        ] {
            assert!(
                PreparedTransfer::new(rpc, &from, &to, 1234, ConfigHashPrefix::new(42)).is_err()
            );
        }
    }
    #[test]
    fn rejects_signatures_for_different_message_or_encoding() {
        let (key, from, to) = fixture();
        let old = prepare(&from, &to);
        let signature = key.sign(old.signing_message()).to_bytes();
        let changed =
            PreparedTransfer::new(TESTNET_RPC, &from, &to, 1235, ConfigHashPrefix::new(42))
                .unwrap();
        assert!(
            changed
                .assemble(&[ExternalSignature {
                    public_address: &from,
                    signature: &signature
                }])
                .is_err()
        );
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(old.signing_message());
        let signature = key.sign(encoded.as_bytes()).to_bytes();
        assert!(
            old.assemble(&[ExternalSignature {
                public_address: &from,
                signature: &signature
            }])
            .is_err()
        );
    }
}
