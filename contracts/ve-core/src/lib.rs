#![no_std]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
//! Shared Validation Engine logic.
//!
//! Used by:
//! - `validation-engine` — injectable `registry` in constructor (tests)
//! - `trustline-oracle-ve` — hardcoded Trustline registry (production)

pub mod clients;
mod events;

use soroban_sdk::{contracterror, contracttype, Address, Bytes, BytesN, Env, String, Vec};
use trustline_sdk::intent::{final_tx_id, intent_id};
use trustline_sdk::types::ValidationMode;

use clients::{SanctionsListClient, TrustlineRegistryClient};
use events::{ConfigInit, ConfigUpdated, TxAdded, TxApproval, TxExecuted, TxPending};

pub use clients::ValidationOracleClient;

/// Transaction validation states.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TxState {
    Approved = 0,
    Rejected = 1,
    Expired = 2,
    Pending = 3,
    Unknown = 4,
}

impl From<TxState> for u32 {
    fn from(value: TxState) -> Self {
        value as u32
    }
}

const WEEK_OF_LEDGERS: u32 = 120_960;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Registry,
    Auditor,
    TrustlineOracleEnabled,
    SanctionsOracleEnabled,
    SanctionsList,
    SanctionsKey,
    AutoValiditySecs,
    ManualValiditySecs,
    MaxSkewSecs,
    Tx(BytesN<32>),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TxMetadata {
    pub created_at: u64,
    pub valid_until: u64,
    pub state: TxState,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    ValidationExpired = 2,
    ValidationTooEarly = 3,
    AlreadySubmitted = 4,
    NotApproved = 5,
    Sanctioned = 6,
    NotPending = 7,
    SanctionsKeyUnknown = 8,
}

pub fn init(
    env: &Env,
    admin: Address,
    registry: Address,
    auto_validity_secs: u64,
    manual_validity_secs: u64,
    max_skew_secs: u64,
) {
    env.storage().instance().set(&DataKey::Admin, &admin);
    env.storage().instance().set(&DataKey::Registry, &registry);
    env.storage().instance().set(&DataKey::Auditor, &admin);
    env.storage()
        .instance()
        .set(&DataKey::TrustlineOracleEnabled, &true);
    env.storage()
        .instance()
        .set(&DataKey::SanctionsOracleEnabled, &false);
    env.storage()
        .instance()
        .set(&DataKey::AutoValiditySecs, &auto_validity_secs);
    env.storage()
        .instance()
        .set(&DataKey::ManualValiditySecs, &manual_validity_secs);
    env.storage()
        .instance()
        .set(&DataKey::MaxSkewSecs, &max_skew_secs);

    ConfigInit {
        admin,
        registry,
        auto_validity_secs,
        manual_validity_secs,
    }
    .publish(env);
}

pub fn admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn registry(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Registry).unwrap()
}

pub fn transfer_admin(env: &Env, new_admin: Address) {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    env.storage().instance().set(&DataKey::Admin, &new_admin);
    bump_instance(env);
}

pub fn set_auditor(env: &Env, new_auditor: Address) {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    env.storage()
        .instance()
        .set(&DataKey::Auditor, &new_auditor);
    bump_instance(env);
}

/// Configure Trustline / sanctions flags; resolves sanctions list via registry key.
pub fn set_validation_configuration(
    env: &Env,
    trustline_enabled: bool,
    sanctions_enabled: bool,
    sanctions_key: Option<String>,
) -> Result<(), Error> {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();

    env.storage()
        .instance()
        .set(&DataKey::TrustlineOracleEnabled, &trustline_enabled);
    env.storage()
        .instance()
        .set(&DataKey::SanctionsOracleEnabled, &sanctions_enabled);

    if sanctions_enabled {
        let key = sanctions_key.ok_or(Error::SanctionsKeyUnknown)?;
        let registry_addr: Address = env.storage().instance().get(&DataKey::Registry).unwrap();
        let reg = TrustlineRegistryClient::new(env, &registry_addr);
        let list = reg.get_record(&key).ok_or(Error::SanctionsKeyUnknown)?;
        env.storage().instance().set(&DataKey::SanctionsList, &list);
        env.storage().instance().set(&DataKey::SanctionsKey, &key);
    } else {
        env.storage().instance().remove(&DataKey::SanctionsList);
        env.storage().instance().remove(&DataKey::SanctionsKey);
    }

    bump_instance(env);
    ConfigUpdated {
        trustline_enabled,
        sanctions_enabled,
    }
    .publish(env);
    Ok(())
}

pub fn upgrade(env: &Env, new_wasm_hash: BytesN<32>) {
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    admin.require_auth();
    env.deployer().update_current_contract_wasm(new_wasm_hash);
}

pub fn version(_env: &Env) -> u32 {
    1
}

pub fn compute_intent_id(
    env: &Env,
    mode: ValidationMode,
    sender: Address,
    protocol: Address,
    value: i128,
    data: Bytes,
) -> BytesN<32> {
    intent_id(env, mode, &sender, &protocol, value, &data)
}

/// Register a pre-validated transaction.
///
/// Auth: `oracle.require_auth()` + registry `is_oracle(oracle)`.
pub fn add_tx(
    env: &Env,
    oracle: Address,
    id: BytesN<32>,
    policy_hash: BytesN<32>,
    timestamp: u64,
    approval_required: bool,
) -> Result<(), Error> {
    oracle.require_auth();

    let registry_addr: Address = env.storage().instance().get(&DataKey::Registry).unwrap();
    let reg = TrustlineRegistryClient::new(env, &registry_addr);
    if !reg.is_oracle(&oracle) {
        return Err(Error::Unauthorized);
    }

    let auto_validity: u64 = env
        .storage()
        .instance()
        .get(&DataKey::AutoValiditySecs)
        .unwrap();
    let max_skew: u64 = env.storage().instance().get(&DataKey::MaxSkewSecs).unwrap();
    let now = env.ledger().timestamp();

    if timestamp.saturating_add(auto_validity) <= now {
        return Err(Error::ValidationExpired);
    }
    if timestamp > now.saturating_add(max_skew) {
        return Err(Error::ValidationTooEarly);
    }

    let key = DataKey::Tx(id.clone());
    if let Some(existing) = env.storage().temporary().get::<_, TxMetadata>(&key) {
        if existing.created_at == timestamp {
            return Err(Error::AlreadySubmitted);
        }
    }

    let meta = if approval_required {
        TxMetadata {
            created_at: timestamp,
            valid_until: 0,
            state: TxState::Pending,
        }
    } else {
        TxMetadata {
            created_at: timestamp,
            valid_until: timestamp.saturating_add(auto_validity),
            state: TxState::Approved,
        }
    };

    env.storage().temporary().set(&key, &meta);
    extend_proof_ttl(env, &key, auto_validity);

    let final_id = final_tx_id(env, &id, timestamp);
    TxAdded {
        final_id: final_id.clone(),
        policy_hash,
    }
    .publish(env);

    if approval_required {
        TxPending {
            final_id,
            id,
        }
        .publish(env);
    }

    bump_instance(env);
    Ok(())
}

pub fn approve_or_reject_tx(env: &Env, id: BytesN<32>, decision: bool) -> Result<(), Error> {
    let auditor: Address = env.storage().instance().get(&DataKey::Auditor).unwrap();
    auditor.require_auth();

    let key = DataKey::Tx(id.clone());
    let mut meta: TxMetadata = env
        .storage()
        .temporary()
        .get(&key)
        .ok_or(Error::NotPending)?;
    if meta.state != TxState::Pending {
        return Err(Error::NotPending);
    }

    let final_id = final_tx_id(env, &id, meta.created_at);
    TxApproval {
        final_id,
        decision,
    }
    .publish(env);

    if decision {
        let manual: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ManualValiditySecs)
            .unwrap();
        meta.valid_until = env.ledger().timestamp().saturating_add(manual);
        meta.state = TxState::Approved;
        env.storage().temporary().set(&key, &meta);
        extend_proof_ttl(env, &key, manual);
    } else {
        env.storage().temporary().remove(&key);
    }

    bump_instance(env);
    Ok(())
}

pub fn require_trustline(
    env: &Env,
    protocol: Address,
    sender: Address,
    value: i128,
    data: Bytes,
) -> Result<(), Error> {
    let addresses = Vec::new(env);
    require_trustline_adv(
        env,
        protocol,
        ValidationMode::Dapp,
        sender,
        value,
        data,
        addresses,
    )
}

pub fn require_trustline_addrs(
    env: &Env,
    protocol: Address,
    sender: Address,
    value: i128,
    data: Bytes,
    addresses: Vec<Address>,
) -> Result<(), Error> {
    require_trustline_adv(
        env,
        protocol,
        ValidationMode::Dapp,
        sender,
        value,
        data,
        addresses,
    )
}

pub fn require_trustline_adv(
    env: &Env,
    protocol: Address,
    mode: ValidationMode,
    sender: Address,
    value: i128,
    data: Bytes,
    addresses: Vec<Address>,
) -> Result<(), Error> {
    protocol.require_auth();
    let id = intent_id(env, mode, &sender, &protocol, value, &data);
    validate_and_consume(env, &sender, &id, &addresses)
}

pub fn check_trustline_status(
    env: &Env,
    protocol: Address,
    sender: Address,
    value: i128,
    data: Bytes,
) -> bool {
    let addresses = Vec::new(env);
    check_status_adv(
        env,
        protocol,
        ValidationMode::Dapp,
        sender,
        value,
        data,
        addresses,
    )
}

pub fn check_status_addrs(
    env: &Env,
    protocol: Address,
    sender: Address,
    value: i128,
    data: Bytes,
    addresses: Vec<Address>,
) -> bool {
    check_status_adv(
        env,
        protocol,
        ValidationMode::Dapp,
        sender,
        value,
        data,
        addresses,
    )
}

pub fn check_status_adv(
    env: &Env,
    protocol: Address,
    mode: ValidationMode,
    sender: Address,
    value: i128,
    data: Bytes,
    addresses: Vec<Address>,
) -> bool {
    let id = intent_id(env, mode, &sender, &protocol, value, &data);
    get_tx_state_inner(env, &sender, &id, &addresses) == TxState::Approved
}

pub fn get_tx_state(
    env: &Env,
    sender: Address,
    id: BytesN<32>,
    addresses: Vec<Address>,
) -> TxState {
    get_tx_state_inner(env, &sender, &id, &addresses)
}

fn validate_and_consume(
    env: &Env,
    sender: &Address,
    id: &BytesN<32>,
    addresses: &Vec<Address>,
) -> Result<(), Error> {
    if check_sanctions(env, sender, addresses) == TxState::Rejected {
        return Err(Error::Sanctioned);
    }
    consume_validation(env, id)
}

fn consume_validation(env: &Env, id: &BytesN<32>) -> Result<(), Error> {
    // When the Trustline oracle is disabled, sanctions are the only gate.
    // There is no oracle proof to read or consume.
    if !trustline_oracle_enabled(env) {
        let final_id = final_tx_id(env, id, 0);
        TxExecuted { final_id }.publish(env);
        bump_instance(env);
        return Ok(());
    }

    if check_oracle_state(env, id) != TxState::Approved {
        return Err(Error::NotApproved);
    }

    let key = DataKey::Tx(id.clone());
    let meta: TxMetadata = env.storage().temporary().get(&key).unwrap();
    let final_id = final_tx_id(env, id, meta.created_at);

    TxExecuted { final_id }.publish(env);

    env.storage().temporary().remove(&key);
    bump_instance(env);
    Ok(())
}

fn get_tx_state_inner(
    env: &Env,
    sender: &Address,
    id: &BytesN<32>,
    addresses: &Vec<Address>,
) -> TxState {
    let sanctions = check_sanctions(env, sender, addresses);
    if sanctions != TxState::Approved {
        return sanctions;
    }
    check_oracle_state(env, id)
}

fn check_oracle_state(env: &Env, id: &BytesN<32>) -> TxState {
    if !trustline_oracle_enabled(env) {
        return TxState::Approved;
    }

    let key = DataKey::Tx(id.clone());
    let Some(meta) = env.storage().temporary().get::<_, TxMetadata>(&key) else {
        return TxState::Unknown;
    };
    if meta.state != TxState::Approved {
        return meta.state;
    }
    if meta.valid_until > env.ledger().timestamp() {
        TxState::Approved
    } else {
        TxState::Expired
    }
}

pub fn trustline_oracle_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TrustlineOracleEnabled)
        .unwrap_or(true)
}

pub fn sanctions_oracle_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::SanctionsOracleEnabled)
        .unwrap_or(false)
}

fn check_sanctions(env: &Env, sender: &Address, addresses: &Vec<Address>) -> TxState {
    if !sanctions_oracle_enabled(env) {
        return TxState::Approved;
    }

    let list: Address = env
        .storage()
        .instance()
        .get(&DataKey::SanctionsList)
        .unwrap();
    let client = SanctionsListClient::new(env, &list);

    if client.is_sanctioned(sender) {
        return TxState::Rejected;
    }
    for addr in addresses.iter() {
        if client.is_sanctioned(&addr) {
            return TxState::Rejected;
        }
    }
    TxState::Approved
}

fn extend_proof_ttl(env: &Env, key: &DataKey, validity_secs: u64) {
    let ledgers = ((validity_secs / 5) as u32).saturating_add(120).max(100);
    env.storage().temporary().extend_ttl(key, ledgers, ledgers);
}

fn bump_instance(env: &Env) {
    let max = env.storage().max_ttl();
    env.storage()
        .instance()
        .extend_ttl(max.saturating_sub(WEEK_OF_LEDGERS), max);
}
