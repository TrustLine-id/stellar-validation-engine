// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use soroban_sdk::{contractevent, Address, BytesN};

#[contractevent(topics = ["cfg", "init"], data_format = "vec")]
pub struct ConfigInit {
    pub admin: Address,
    pub registry: Address,
    pub auto_validity_secs: u64,
    pub manual_validity_secs: u64,
}

#[contractevent(topics = ["cfg", "upd"], data_format = "vec")]
pub struct ConfigUpdated {
    pub trustline_enabled: bool,
    pub sanctions_enabled: bool,
}

#[contractevent(topics = ["tx", "added"], data_format = "vec")]
pub struct TxAdded {
    pub final_id: BytesN<32>,
    pub policy_hash: BytesN<32>,
}

#[contractevent(topics = ["tx", "pend"], data_format = "vec")]
pub struct TxPending {
    pub final_id: BytesN<32>,
    pub id: BytesN<32>,
}

#[contractevent(topics = ["tx", "appr"], data_format = "vec")]
pub struct TxApproval {
    pub final_id: BytesN<32>,
    pub decision: bool,
}

#[contractevent(topics = ["tx", "exec"], data_format = "single-value")]
pub struct TxExecuted {
    pub final_id: BytesN<32>,
}
