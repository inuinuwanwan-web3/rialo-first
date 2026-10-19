//! No key generation, signing, RPC client or persistence in these tests.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
const FROM: &str = "7viL6ErV1ejDeRKbztqvmUZF9EnNgJ5q14yEMhEsiSQa";
const TO: &str = "FAyTs54yLpZ4KRHot8dVpJaoJJAibwkfEi9ck3NjHVc6";
fn prepared(amount: u64, time: i64, prefix: u64) -> PreparedTransfer {
    PreparedTransfer::new_with_amount(
        TESTNET_RPC,
        FROM,
        TO,
        time,
        ConfigHashPrefix::new(prefix),
        amount,
    )
    .unwrap()
}
#[test]
fn unsigned_fields_and_canonical_wire() {
    let p = prepared(1, 10000, 42);
    assert_eq!(p.transaction.signatures, vec![vec![0; 64]]);
    assert!(!p.transaction.is_fully_signed());
    let VersionedMessage::Legacy(m) = bincode::deserialize(p.signing_message()).unwrap() else {
        panic!("unsupported");
    };
    assert_eq!(m.header.num_required_signatures, 1);
    assert_eq!(m.account_keys[0].to_string(), FROM);
    assert_eq!(m.valid_from, 10000);
    assert_eq!(m.config_hash_prefix, ConfigHashPrefix::new(42));
    assert_eq!(m.instructions.len(), 1);
    let ix = &m.instructions[0];
    assert_eq!(
        m.account_keys[ix.program_id_index as usize].to_string(),
        "11111111111111111111111111111111"
    );
    assert_eq!(m.account_keys[ix.accounts[0] as usize].to_string(), FROM);
    assert_eq!(m.account_keys[ix.accounts[1] as usize].to_string(), TO);
    assert_eq!(&ix.data[..4], &2u32.to_le_bytes());
    assert_eq!(u64::from_le_bytes(ix.data[4..].try_into().unwrap()), 1);
    let wire = p.transaction.serialize_wired().unwrap();
    assert_eq!(wire[0], 1);
    assert_eq!(&wire[1..65], &[0; 64]);
    assert_eq!(&wire[65..], p.signing_message());
    let restored = Transaction::deserialize_wired(&wire).unwrap();
    assert_eq!(restored.serialize_wired().unwrap(), wire);
    assert!(!restored.is_fully_signed());
}
#[test]
fn deterministic_and_sensitive_to_all_transaction_inputs() {
    let p = prepared(1, 10000, 42);
    assert_eq!(
        p.signing_message(),
        prepared(1, 10000, 42).signing_message()
    );
    for different in [
        prepared(2, 10000, 42),
        prepared(1, 10001, 42),
        prepared(1, 10000, 43),
    ] {
        assert_ne!(p.signing_message(), different.signing_message());
    }
    let other = PreparedTransfer::new_with_amount(
        TESTNET_RPC,
        FROM,
        &Pubkey::from([2; 32]).to_string(),
        10000,
        ConfigHashPrefix::new(42),
        1,
    )
    .unwrap();
    assert_ne!(p.signing_message(), other.signing_message());
}
#[test]
fn fee_payload_is_message_not_unsigned_envelope() {
    let p = prepared(1, 10000, 42);
    let message = p.signing_message();
    let encoded = STANDARD.encode(message);
    let decoded = STANDARD.decode(encoded).unwrap();
    let versioned: VersionedMessage = bincode::deserialize(&decoded).unwrap();
    assert_eq!(versioned.serialize_safe().unwrap(), message);
    let wire = p.transaction.serialize_wired().unwrap();
    assert_ne!(STANDARD.encode(wire), STANDARD.encode(message));
    // Required signer count remains one without any real signature.
    let VersionedMessage::Legacy(m) = versioned else {
        panic!("unsupported");
    };
    assert_eq!(m.header.num_required_signatures, 1);
}
#[test]
fn malformed_unsigned_envelopes_rejected() {
    let p = prepared(1, 10000, 42);
    for bad in [Vec::new(), vec![1], vec![1; 32]] {
        assert!(Transaction::deserialize_wired(&bad).is_err());
    }
    let mut tx = Transaction::new(p.signing_message().to_vec(), 1);
    tx.signatures[0].truncate(63);
    assert!(tx.serialize_wired().is_err());
}
#[test]
fn invalid_or_missing_address_and_network_rejected() {
    for (network, from, to, amount) in [
        (TESTNET_RPC, "", TO, 1),
        (TESTNET_RPC, FROM, "", 1),
        (TESTNET_RPC, "invalid", TO, 1),
        (TESTNET_RPC, FROM, FROM, 1),
        (TESTNET_RPC, FROM, TO, 0),
        ("https://wrong.invalid/", FROM, TO, 1),
    ] {
        assert!(
            PreparedTransfer::new_with_amount(
                network,
                from,
                to,
                10000,
                ConfigHashPrefix::new(42),
                amount
            )
            .is_err()
        );
    }
}
