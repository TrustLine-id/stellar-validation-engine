// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Trustline Digital Asset Ltd.
use soroban_sdk::{contractevent, Address, BytesN, String};

#[contractevent(topics = ["reg", "init"], data_format = "single-value")]
pub struct RegistryInit {
    pub admin: Address,
}

#[contractevent(topics = ["reg", "admin"], data_format = "vec")]
pub struct RegistryAdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
}

#[contractevent(topics = ["reg", "oracle"], data_format = "vec")]
pub struct RegistryOracleUpdated {
    pub oracle: Address,
    pub approved: bool,
}

#[contractevent(topics = ["reg", "record"], data_format = "vec")]
pub struct RegistryRecordSet {
    pub key: String,
    pub hashed: BytesN<32>,
    pub addr: Address,
}

#[contractevent(topics = ["reg", "rm"], data_format = "vec")]
pub struct RegistryRecordRemoved {
    pub key: String,
    pub hashed: BytesN<32>,
}
