// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
//! CPI clients for Validation Engine internals and oracle/backend consumers.
//!
//! - Registry / sanctions: used by `ve-core` only
//! - `ValidationOracleClient`: oracle/admin surface — not in the SDK

use soroban_sdk::{contractclient, Address, Bytes, BytesN, Env, String, Vec};

use crate::TxState;
use trustline_sdk::types::ValidationMode;

/// Minimal registry surface for VE CPI (`is_oracle`, `get_record`).
#[contractclient(name = "TrustlineRegistryClient")]
pub trait TrustlineRegistry {
    fn is_oracle(env: Env, oracle: Address) -> bool;
    fn get_record(env: Env, key: String) -> Option<Address>;
}

/// Chainalysis-compatible sanctions list (`is_sanctioned`).
#[contractclient(name = "SanctionsListClient")]
pub trait SanctionsList {
    fn is_sanctioned(env: Env, addr: Address) -> bool;
}

/// Oracle / admin / backend surface (`add_tx`, `get_tx_state`, feature flags, …).
///
/// Integrators should use `trustline_sdk::ValidationEngineClient` (`require_*` / `check_*`) instead.
#[contractclient(name = "ValidationOracleClient")]
pub trait ValidationOracle {
    fn add_tx(
        env: Env,
        oracle: Address,
        id: BytesN<32>,
        policy_hash: BytesN<32>,
        timestamp: u64,
        approval_required: bool,
    );

    fn approve_or_reject_tx(env: Env, id: BytesN<32>, decision: bool);

    fn get_tx_state(env: Env, sender: Address, id: BytesN<32>, addresses: Vec<Address>) -> TxState;

    fn trustline_oracle_enabled(env: Env) -> bool;

    fn sanctions_oracle_enabled(env: Env) -> bool;

    fn registry(env: Env) -> Address;

    fn compute_intent_id(
        env: Env,
        mode: ValidationMode,
        sender: Address,
        protocol: Address,
        value: i128,
        data: Bytes,
    ) -> BytesN<32>;

    fn version(env: Env) -> u32;
}
