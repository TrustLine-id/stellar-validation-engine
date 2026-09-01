#![no_std]

// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
//! Trustline Oracle Validation Engine.
//!
//! Same surface as `validation-engine`, but the Trustline registry address is
//! baked into the WASM via [`registry_address::VALIDATION_REGISTRY`].

mod registry_address;

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String, Vec};
use trustline_sdk::types::ValidationMode;
use ve_core::Error;

pub use ve_core::{TxMetadata, TxState, ValidationOracleClient};

#[contract]
pub struct TrustlineOracleVE;

#[contractimpl]
impl TrustlineOracleVE {
    /// Initialize the instance; registry comes from the hardcoded constant.
    pub fn __constructor(
        env: Env,
        admin: Address,
        auto_validity_secs: u64,
        manual_validity_secs: u64,
        max_skew_secs: u64,
    ) {
        let registry = Address::from_str(&env, registry_address::VALIDATION_REGISTRY);
        ve_core::init(
            &env,
            admin,
            registry,
            auto_validity_secs,
            manual_validity_secs,
            max_skew_secs,
        );
    }

    pub fn admin(env: Env) -> Address {
        ve_core::admin(&env)
    }

    pub fn registry(env: Env) -> Address {
        ve_core::registry(&env)
    }

    pub fn trustline_oracle_enabled(env: Env) -> bool {
        ve_core::trustline_oracle_enabled(&env)
    }

    pub fn sanctions_oracle_enabled(env: Env) -> bool {
        ve_core::sanctions_oracle_enabled(&env)
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        ve_core::transfer_admin(&env, new_admin)
    }

    pub fn set_auditor(env: Env, new_auditor: Address) {
        ve_core::set_auditor(&env, new_auditor)
    }

    pub fn set_validation_configuration(
        env: Env,
        trustline_enabled: bool,
        sanctions_enabled: bool,
        sanctions_key: Option<String>,
    ) -> Result<(), Error> {
        ve_core::set_validation_configuration(
            &env,
            trustline_enabled,
            sanctions_enabled,
            sanctions_key,
        )
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        ve_core::upgrade(&env, new_wasm_hash)
    }

    pub fn version(env: Env) -> u32 {
        ve_core::version(&env)
    }

    pub fn compute_intent_id(
        env: Env,
        mode: ValidationMode,
        sender: Address,
        protocol: Address,
        value: i128,
        data: Bytes,
    ) -> BytesN<32> {
        ve_core::compute_intent_id(&env, mode, sender, protocol, value, data)
    }

    pub fn add_tx(
        env: Env,
        oracle: Address,
        id: BytesN<32>,
        policy_hash: BytesN<32>,
        timestamp: u64,
        approval_required: bool,
    ) -> Result<(), Error> {
        ve_core::add_tx(&env, oracle, id, policy_hash, timestamp, approval_required)
    }

    pub fn approve_or_reject_tx(env: Env, id: BytesN<32>, decision: bool) -> Result<(), Error> {
        ve_core::approve_or_reject_tx(&env, id, decision)
    }

    pub fn require_trustline(
        env: Env,
        protocol: Address,
        sender: Address,
        value: i128,
        data: Bytes,
    ) -> Result<(), Error> {
        ve_core::require_trustline(&env, protocol, sender, value, data)
    }

    pub fn require_trustline_addrs(
        env: Env,
        protocol: Address,
        sender: Address,
        value: i128,
        data: Bytes,
        addresses: Vec<Address>,
    ) -> Result<(), Error> {
        ve_core::require_trustline_addrs(&env, protocol, sender, value, data, addresses)
    }

    pub fn require_trustline_adv(
        env: Env,
        protocol: Address,
        mode: ValidationMode,
        sender: Address,
        value: i128,
        data: Bytes,
        addresses: Vec<Address>,
    ) -> Result<(), Error> {
        ve_core::require_trustline_adv(&env, protocol, mode, sender, value, data, addresses)
    }

    pub fn check_trustline_status(
        env: Env,
        protocol: Address,
        sender: Address,
        value: i128,
        data: Bytes,
    ) -> bool {
        ve_core::check_trustline_status(&env, protocol, sender, value, data)
    }

    pub fn check_status_addrs(
        env: Env,
        protocol: Address,
        sender: Address,
        value: i128,
        data: Bytes,
        addresses: Vec<Address>,
    ) -> bool {
        ve_core::check_status_addrs(&env, protocol, sender, value, data, addresses)
    }

    pub fn check_status_adv(
        env: Env,
        protocol: Address,
        mode: ValidationMode,
        sender: Address,
        value: i128,
        data: Bytes,
        addresses: Vec<Address>,
    ) -> bool {
        ve_core::check_status_adv(&env, protocol, mode, sender, value, data, addresses)
    }

    pub fn get_tx_state(
        env: Env,
        sender: Address,
        id: BytesN<32>,
        addresses: Vec<Address>,
    ) -> TxState {
        ve_core::get_tx_state(&env, sender, id, addresses)
    }
}
