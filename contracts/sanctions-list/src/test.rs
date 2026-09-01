#![cfg(test)]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn set_and_check_sanction() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let target = Address::generate(&env);
    let contract_id = env.register(SanctionsList, (&owner,));
    let client = SanctionsListClient::new(&env, &contract_id);

    assert!(!client.is_sanctioned(&target));
    client.set_sanction(&target, &true);
    assert!(client.is_sanctioned(&target));
    client.set_sanction(&target, &false);
    assert!(!client.is_sanctioned(&target));
}
