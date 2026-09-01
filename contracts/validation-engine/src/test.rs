#![cfg(test)]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, Bytes, BytesN, Env, IntoVal, String,
};
use trustline_registry::TrustlineRegistry;
use trustline_sdk::{intent_id, ValidationMode};

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    ValidationEngineClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let protocol = Address::generate(&env);

    let registry_id = env.register(TrustlineRegistry, (&admin,));
    let registry = trustline_registry::TrustlineRegistryClient::new(&env, &registry_id);
    registry.set_oracle(&oracle, &true);

    let ve_id = env.register(
        ValidationEngine,
        (&admin, &registry_id, 1800_u64, 432000_u64, 60_u64),
    );
    let client = ValidationEngineClient::new(&env, &ve_id);
    (env, admin, oracle, protocol, client)
}

fn upload_validation_engine_wasm(env: &Env) -> BytesN<32> {
    let wasm = Bytes::from_slice(
        env,
        include_bytes!("../../../target/wasm32v1-none/release/validation_engine.wasm"),
    );
    env.deployer().upload_contract_wasm(wasm)
}

#[test]
fn upgrade_requires_admin_auth() {
    let (env, admin, _oracle, _protocol, client) = setup();
    let wasm_hash = upload_validation_engine_wasm(&env);
    let contract_id = client.address.clone();

    env.set_auths(&[]);
    assert!(client.try_upgrade(&wasm_hash).is_err());

    client
        .mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "upgrade",
                args: (wasm_hash.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .upgrade(&wasm_hash);
}

#[test]
fn upgrade_preserves_storage_and_add_consume_cycle() {
    let (env, admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 77;
    let data = Bytes::from_slice(&env, b"post-upgrade");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[11u8; 32]);
    let ts = env.ledger().timestamp();
    let registry_id = client.registry();

    client.add_tx(&oracle, &id, &policy, &ts, &false);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Approved
    );

    let wasm_hash = upload_validation_engine_wasm(&env);
    client.upgrade(&wasm_hash);

    assert_eq!(client.admin(), admin);
    assert_eq!(client.registry(), registry_id);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Approved
    );

    client.require_trustline(&protocol, &sender, &value, &data);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Unknown
    );
    assert!(client
        .try_require_trustline(&protocol, &sender, &value, &data)
        .is_err());
}

#[test]
fn add_tx_then_require_trustline_consumes_once() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 100;
    let data = Bytes::from_slice(&env, b"pay");

    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[0u8; 32]);
    let ts = env.ledger().timestamp();

    client.add_tx(&oracle, &id, &policy, &ts, &false);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Approved
    );

    client.require_trustline(&protocol, &sender, &value, &data);

    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Unknown
    );

    let second = client.try_require_trustline(&protocol, &sender, &value, &data);
    assert!(second.is_err());
}

#[test]
fn add_tx_rejects_non_oracle() {
    let (env, _admin, _oracle, protocol, client) = setup();
    let impostor = Address::generate(&env);
    let sender = Address::generate(&env);
    let value: i128 = 1;
    let data = Bytes::from_slice(&env, b"x");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[7u8; 32]);
    assert!(client
        .try_add_tx(&impostor, &id, &policy, &env.ledger().timestamp(), &false)
        .is_err());
}

#[test]
fn expired_proof_is_rejected() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 0;
    let data = Bytes::from_slice(&env, b"x");

    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[1u8; 32]);
    let ts = env.ledger().timestamp();
    client.add_tx(&oracle, &id, &policy, &ts, &false);

    env.ledger().with_mut(|l| {
        l.timestamp = ts + 1801;
    });

    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Expired
    );
    assert!(!client.check_trustline_status(&protocol, &sender, &value, &data));
    assert!(client
        .try_require_trustline(&protocol, &sender, &value, &data)
        .is_err());
}

#[test]
fn require_trustline_fails_without_prevalidation() {
    let (env, _admin, _oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 10;
    let data = Bytes::from_slice(&env, b"unvalidated");

    assert!(client
        .try_require_trustline(&protocol, &sender, &value, &data)
        .is_err());
    assert!(!client.check_trustline_status(&protocol, &sender, &value, &data));
}

#[test]
fn require_trustline_succeeds_without_proof_when_oracle_disabled() {
    let (env, _admin, _oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 10;
    let data = Bytes::from_slice(&env, b"sanctions-only");

    assert!(client.trustline_oracle_enabled());
    assert!(!client.sanctions_oracle_enabled());

    client.set_validation_configuration(&false, &false, &None);

    assert!(!client.trustline_oracle_enabled());
    assert!(!client.sanctions_oracle_enabled());
    assert!(client.check_trustline_status(&protocol, &sender, &value, &data));
    client.require_trustline(&protocol, &sender, &value, &data);
}

#[test]
fn sanctions_still_reject_when_oracle_disabled() {
    let (env, admin, _oracle, protocol, client) = setup();
    let sender = Address::generate(&env);

    let sanctions_id = env.register(sanctions_list::SanctionsList, (&admin,));
    let sanctions = sanctions_list::SanctionsListClient::new(&env, &sanctions_id);
    sanctions.set_sanction(&sender, &true);

    let registry_id = client.registry();
    let registry = trustline_registry::TrustlineRegistryClient::new(&env, &registry_id);
    let sanctions_key = String::from_str(&env, "SANCTIONS_ONLY");
    registry.set_record(&sanctions_key, &sanctions_id);
    client.set_validation_configuration(&false, &true, &Some(sanctions_key));

    assert!(!client.trustline_oracle_enabled());
    assert!(client.sanctions_oracle_enabled());
    let data = Bytes::from_slice(&env, b"sanctions-only");
    assert!(!client.check_trustline_status(&protocol, &sender, &0, &data));
    assert!(client
        .try_require_trustline(&protocol, &sender, &0, &data)
        .is_err());
}

#[test]
fn wrong_intent_cannot_consume_proof() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 50;
    let data = Bytes::from_slice(&env, b"pay");
    let other_data = Bytes::from_slice(&env, b"other");
    let other_protocol = Address::generate(&env);

    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[4u8; 32]);
    client.add_tx(&oracle, &id, &policy, &env.ledger().timestamp(), &false);

    assert!(client
        .try_require_trustline(&protocol, &sender, &value, &other_data)
        .is_err());
    assert!(client
        .try_require_trustline(&protocol, &sender, &(value + 1), &data)
        .is_err());
    assert!(client
        .try_require_trustline(&other_protocol, &sender, &value, &data)
        .is_err());

    client.require_trustline(&protocol, &sender, &value, &data);
}

#[test]
fn add_tx_rejects_exact_replay() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 1;
    let data = Bytes::from_slice(&env, b"replay");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[5u8; 32]);
    let ts = env.ledger().timestamp();

    client.add_tx(&oracle, &id, &policy, &ts, &false);
    assert!(client
        .try_add_tx(&oracle, &id, &policy, &ts, &false)
        .is_err());
}

#[test]
fn sanctions_reject_when_enabled() {
    let (env, admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let bad = Address::generate(&env);

    let sanctions_id = env.register(sanctions_list::SanctionsList, (&admin,));
    let sanctions = sanctions_list::SanctionsListClient::new(&env, &sanctions_id);
    sanctions.set_sanction(&bad, &true);

    let registry_id = client.registry();
    let registry = trustline_registry::TrustlineRegistryClient::new(&env, &registry_id);
    let key = String::from_str(&env, "TRUSTLINE_TEST_ORACLE");
    registry.set_record(&key, &sanctions_id);

    client.set_validation_configuration(&true, &true, &Some(key));

    let value: i128 = 1;
    let data = Bytes::from_slice(&env, b"pay");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[2u8; 32]);
    client.add_tx(&oracle, &id, &policy, &env.ledger().timestamp(), &false);

    let addresses = soroban_sdk::vec![&env, bad.clone()];
    assert_eq!(
        client.get_tx_state(&sender, &id, &addresses),
        TxState::Rejected
    );

    let rejected =
        client.try_require_trustline_addrs(&protocol, &sender, &value, &data, &addresses);
    assert!(rejected.is_err());
}

#[test]
fn pending_then_approve() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 0;
    let data = Bytes::from_slice(&env, b"manual");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[3u8; 32]);
    let ts = env.ledger().timestamp();

    client.add_tx(&oracle, &id, &policy, &ts, &true);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Pending
    );

    client.approve_or_reject_tx(&id, &true);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Approved
    );

    client.require_trustline(&protocol, &sender, &value, &data);
}

#[test]
fn pending_then_reject() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 0;
    let data = Bytes::from_slice(&env, b"manual-reject");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[8u8; 32]);
    let ts = env.ledger().timestamp();

    client.add_tx(&oracle, &id, &policy, &ts, &true);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Pending
    );

    client.approve_or_reject_tx(&id, &false);
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Unknown
    );
    assert!(!client.check_trustline_status(&protocol, &sender, &value, &data));
    assert!(client
        .try_require_trustline(&protocol, &sender, &value, &data)
        .is_err());
}

#[test]
fn add_tx_rejects_timestamp_too_early() {
    let (env, _admin, oracle, protocol, client) = setup();
    let sender = Address::generate(&env);
    let value: i128 = 1;
    let data = Bytes::from_slice(&env, b"early");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[9u8; 32]);
    let now = env.ledger().timestamp();
    // max_skew_secs = 60 in setup()
    let too_early = now + 61;

    assert!(client
        .try_add_tx(&oracle, &id, &policy, &too_early, &false)
        .is_err());
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Unknown
    );
}

#[test]
fn add_tx_rejects_expired_at_registration() {
    let (env, _admin, oracle, protocol, client) = setup();
    env.ledger().with_mut(|l| {
        l.timestamp = 2000;
    });

    let sender = Address::generate(&env);
    let value: i128 = 1;
    let data = Bytes::from_slice(&env, b"late");
    let id = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, value, &data);
    let policy = BytesN::from_array(&env, &[10u8; 32]);
    let now = env.ledger().timestamp();
    // auto_validity_secs = 1800; reject when timestamp + auto_validity <= now
    let expired_at_submit = now.saturating_sub(1800);

    assert!(client
        .try_add_tx(&oracle, &id, &policy, &expired_at_submit, &false)
        .is_err());
    assert_eq!(
        client.get_tx_state(&sender, &id, &Vec::new(&env)),
        TxState::Unknown
    );
}

#[test]
fn intent_id_is_deterministic() {
    let env = Env::default();
    let sender = Address::generate(&env);
    let protocol = Address::generate(&env);
    let data = Bytes::from_slice(&env, b"abc");
    let a = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, 42, &data);
    let b = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, 42, &data);
    assert_eq!(a, b);
    let c = intent_id(&env, ValidationMode::Dapp, &sender, &protocol, 43, &data);
    assert_ne!(a, c);
}
