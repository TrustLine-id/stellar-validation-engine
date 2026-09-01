#![no_std]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
//! Trustline Registry (Soroban).
//!
//! - Oracle allowlist: `set_oracle` / `is_oracle`
//! - Key → address records (sanctions lists, etc.): `set_record` / `get_record`
//! - Enumeration: `get_all_records` via an on-chain key index
//!
//! Keys are hashed with keccak256 of the UTF-8 bytes.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, String, Vec,
};

mod events;
use events::{
    RegistryAdminTransferred, RegistryInit, RegistryOracleUpdated, RegistryRecordRemoved,
    RegistryRecordSet,
};

#[contract]
pub struct TrustlineRegistry;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Oracle(Address),
    Record(BytesN<32>),
    /// Enumeration index of original string keys.
    RecordKeys,
    /// Whether a hashed key was already pushed into `RecordKeys`.
    KeySeen(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    EmptyKey = 2,
}

#[contractimpl]
impl TrustlineRegistry {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::RecordKeys, &Vec::<String>::new(&env));
        bump_persistent(&env, &DataKey::RecordKeys);
        RegistryInit { admin }.publish(&env);
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        bump_instance(&env);
        RegistryAdminTransferred {
            old_admin: admin,
            new_admin,
        }
        .publish(&env);
    }

    /// Grant or revoke oracle publishing rights.
    pub fn set_oracle(env: Env, oracle: Address, approved: bool) {
        require_admin(&env);
        let key = DataKey::Oracle(oracle.clone());
        if approved {
            env.storage().persistent().set(&key, &true);
            bump_persistent(&env, &key);
        } else {
            env.storage().persistent().remove(&key);
        }
        bump_instance(&env);
        RegistryOracleUpdated { oracle, approved }.publish(&env);
    }

    pub fn is_oracle(env: Env, oracle: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Oracle(oracle))
            .unwrap_or(false)
    }

    /// Register an address under a string key.
    pub fn set_record(env: Env, key: String, addr: Address) -> Result<(), Error> {
        require_admin(&env);
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let hashed = hash_key(&env, &key);
        index_key_if_new(&env, &key, &hashed);

        let data_key = DataKey::Record(hashed.clone());
        env.storage().persistent().set(&data_key, &addr);
        bump_persistent(&env, &data_key);
        bump_instance(&env);
        RegistryRecordSet {
            key,
            hashed,
            addr,
        }
        .publish(&env);
        Ok(())
    }

    /// Resolve a key to an address. Returns `None` if unset.
    pub fn get_record(env: Env, key: String) -> Option<Address> {
        if key.is_empty() {
            return None;
        }
        let hashed = hash_key(&env, &key);
        env.storage().persistent().get(&DataKey::Record(hashed))
    }

    /// All live records. Keys whose address was removed are skipped.
    pub fn get_all_records(env: Env) -> (Vec<String>, Vec<Address>) {
        let all_keys: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::RecordKeys)
            .unwrap_or_else(|| Vec::new(&env));
        bump_persistent(&env, &DataKey::RecordKeys);

        let mut keys = Vec::new(&env);
        let mut addrs = Vec::new(&env);
        for key in all_keys.iter() {
            let hashed = hash_key(&env, &key);
            if let Some(addr) = env
                .storage()
                .persistent()
                .get::<_, Address>(&DataKey::Record(hashed))
            {
                keys.push_back(key);
                addrs.push_back(addr);
            }
        }
        (keys, addrs)
    }

    /// Clear a record. The string key remains in the enumeration index and is
    /// filtered out of `get_all_records` when the address is gone.
    pub fn remove_record(env: Env, key: String) -> Result<(), Error> {
        require_admin(&env);
        if key.is_empty() {
            return Err(Error::EmptyKey);
        }
        let hashed = hash_key(&env, &key);
        env.storage()
            .persistent()
            .remove(&DataKey::Record(hashed.clone()));
        bump_instance(&env);
        RegistryRecordRemoved { key, hashed }.publish(&env);
        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn version(_env: Env) -> u32 {
        2
    }
}

fn require_admin(env: &Env) {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
}

fn hash_key(env: &Env, key: &String) -> BytesN<32> {
    let bytes = key.to_bytes();
    env.crypto().keccak256(&bytes).into()
}

/// Push `key` into the enumeration index on first sight.
fn index_key_if_new(env: &Env, key: &String, hashed: &BytesN<32>) {
    let seen_key = DataKey::KeySeen(hashed.clone());
    let already: bool = env.storage().persistent().get(&seen_key).unwrap_or(false);
    if already {
        return;
    }

    let mut keys: Vec<String> = env
        .storage()
        .persistent()
        .get(&DataKey::RecordKeys)
        .unwrap_or_else(|| Vec::new(env));
    keys.push_back(key.clone());
    env.storage().persistent().set(&DataKey::RecordKeys, &keys);
    env.storage().persistent().set(&seen_key, &true);
    bump_persistent(env, &DataKey::RecordKeys);
    bump_persistent(env, &seen_key);
}

fn bump_instance(env: &Env) {
    const WEEK_OF_LEDGERS: u32 = 120_960;
    let max = env.storage().max_ttl();
    env.storage()
        .instance()
        .extend_ttl(max.saturating_sub(WEEK_OF_LEDGERS), max);
}

/// Bump-on-write for long-lived registry entries (oracles, records, index).
fn bump_persistent(env: &Env, key: &DataKey) {
    const WEEK_OF_LEDGERS: u32 = 120_960;
    let max = env.storage().max_ttl();
    env.storage()
        .persistent()
        .extend_ttl(key, max.saturating_sub(WEEK_OF_LEDGERS), max);
}

mod test;
