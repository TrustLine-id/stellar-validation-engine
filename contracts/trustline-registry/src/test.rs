#![cfg(test)]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, Address, TrustlineRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(TrustlineRegistry, (&admin,));
    let client = TrustlineRegistryClient::new(&env, &id);
    (env, admin, client)
}

#[test]
fn oracle_grant_and_revoke() {
    let (env, _admin, client) = setup();
    let oracle = Address::generate(&env);
    assert!(!client.is_oracle(&oracle));
    client.set_oracle(&oracle, &true);
    assert!(client.is_oracle(&oracle));
    client.set_oracle(&oracle, &false);
    assert!(!client.is_oracle(&oracle));
}

#[test]
fn record_set_get_remove() {
    let (env, _admin, client) = setup();
    let list = Address::generate(&env);
    let key = String::from_str(&env, "TRUSTLINE_TEST_ORACLE");

    assert_eq!(client.get_record(&key), None);
    client.set_record(&key, &list);
    assert_eq!(client.get_record(&key), Some(list.clone()));
    client.remove_record(&key);
    assert_eq!(client.get_record(&key), None);
}

#[test]
fn get_all_records_lists_live_keys_only() {
    let (env, _admin, client) = setup();
    let list_a = Address::generate(&env);
    let list_b = Address::generate(&env);
    let key_a = String::from_str(&env, "SANCTIONS_A");
    let key_b = String::from_str(&env, "SANCTIONS_B");

    let (keys0, addrs0) = client.get_all_records();
    assert_eq!(keys0.len(), 0);
    assert_eq!(addrs0.len(), 0);

    client.set_record(&key_a, &list_a);
    client.set_record(&key_b, &list_b);
    // Update must not duplicate the key in the index.
    client.set_record(&key_a, &list_a);

    let (keys, addrs) = client.get_all_records();
    assert_eq!(keys.len(), 2);
    assert_eq!(addrs.len(), 2);
    assert_eq!(keys.get(0).unwrap(), key_a);
    assert_eq!(addrs.get(0).unwrap(), list_a);
    assert_eq!(keys.get(1).unwrap(), key_b);
    assert_eq!(addrs.get(1).unwrap(), list_b);

    client.remove_record(&key_a);
    let (keys2, addrs2) = client.get_all_records();
    assert_eq!(keys2.len(), 1);
    assert_eq!(addrs2.len(), 1);
    assert_eq!(keys2.get(0).unwrap(), key_b);
    assert_eq!(addrs2.get(0).unwrap(), list_b);
}
