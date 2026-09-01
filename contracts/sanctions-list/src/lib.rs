#![no_std]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
//! Reference sanctions list — Chainalysis-compatible.
//!
//! `is_sanctioned(addr) -> bool`
//!
//! No Chainalysis deployment exists on Stellar yet; any oracle that exposes this
//! method can be plugged into the Validation Engine after Trustline registers it
//! in the registry (`set_record`) and the VE admin enables it via
//! `set_validation_configuration(..., sanctions_key=…)`.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

mod events;
use events::{OwnerUpdated, SanctionUpdated};

#[contract]
pub struct SanctionsList;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Owner,
    Sanctioned(Address),
}

#[contractimpl]
impl SanctionsList {
    pub fn __constructor(env: Env, owner: Address) {
        env.storage().instance().set(&DataKey::Owner, &owner);
    }

    pub fn owner(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Owner).unwrap()
    }

    pub fn set_owner(env: Env, new_owner: Address) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        env.storage().instance().set(&DataKey::Owner, &new_owner);
        OwnerUpdated {
            old_owner: owner,
            new_owner,
        }
        .publish(&env);
    }

    /// Mark or clear an address as sanctioned.
    pub fn set_sanction(env: Env, addr: Address, sanctioned: bool) {
        let owner: Address = env.storage().instance().get(&DataKey::Owner).unwrap();
        owner.require_auth();
        let key = DataKey::Sanctioned(addr.clone());
        if sanctioned {
            env.storage().persistent().set(&key, &true);
        } else {
            env.storage().persistent().remove(&key);
        }
        SanctionUpdated {
            addr,
            sanctioned,
        }
        .publish(&env);
    }

    /// Chainalysis-compatible `is_sanctioned(address)`.
    pub fn is_sanctioned(env: Env, addr: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Sanctioned(addr))
            .unwrap_or(false)
    }
}

mod test;
