// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use soroban_sdk::{contractevent, Address};

#[contractevent(topics = ["own", "upd"], data_format = "vec")]
pub struct OwnerUpdated {
    pub old_owner: Address,
    pub new_owner: Address,
}

#[contractevent(topics = ["sanc", "upd"], data_format = "vec")]
pub struct SanctionUpdated {
    pub addr: Address,
    pub sanctioned: bool,
}
