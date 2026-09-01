# Trustline Stellar (Soroban) Validation Engine

> Part of the Trustline Stellar / Soroban stack, developed with support from the [Stellar Community Fund](https://communityfund.stellar.org) (**SCF #44**).

## Overview

The Trustline Validation Engine is a set of Soroban smart contracts for building compliance and validation solutions on Stellar. It provides a modular framework for transaction validation, sanctions screening, and registry management, with instance upgrades and oracle integration. The system is designed for use cases requiring robust compliance, such as regulated DeFi, on-chain KYC/AML, and enterprise blockchain applications.

## Security

Trust model, key custody, replay protection and known limitations are documented in
[SECURITY.md](SECURITY.md). Report vulnerabilities privately to security@trustline.id.
These contracts have not been externally audited yet.

## Features

- **Validation Engine** — Core logic for transaction validation, supporting automatic and manual (auditor) approval flows
- **Sanctions List** — Interface and reference implementation for on-chain sanctions screening
- **Trustline Registry** — Shared registry for oracle allowlisting and key → address records
- **Upgradeable instances** — Per-client deploy; `upgrade(new_wasm_hash)` with `admin.require_auth()`
- **Implementations** — Injectable `validation-engine` (tests / custom deploys) and production `trustline-oracle-ve` (hardcoded registry)

## Directory Structure

```
contracts/
  trustline-registry/   # Shared TL registry (oracles + set_record / get_record / get_all_records)
  ve-core/              # Shared VE logic + CPI clients (lib only)
  validation-engine/    # Injectable registry constructor (tests / generic deploys)
  trustline-oracle-ve/  # Production VE (hardcoded VALIDATION_REGISTRY in WASM)
  sanctions-list/       # Reference sanctions list (is_sanctioned)
```

Shared intent hashing and `ValidationMode` live in the published [`trustline-sdk`](https://crates.io/crates/trustline-sdk) crate (repo: [`stellar-sdk`](https://github.com/TrustLine-id/stellar-sdk)). `TxState` and `ValidationOracleClient` live in this package.

## Main Contracts

### ve-core

Shared library implementing the oracle-based validation logic used by both engine facades:

- Transaction metadata tracking (creation, expiry, state)
- Oracle pre-validation (`add_tx`) and one-shot consumption (`require_trustline*`)
- Sanctions screening via a registry-resolved list contract
- Admin / auditor configuration
- Intent id hashing (via `trustline-sdk`)

Also exports `ValidationOracleClient` — the oracle / backend / admin CPI surface
(`add_tx`, `approve_or_reject_tx`, `get_tx_state`, feature-flag getters, `registry`,
`compute_intent_id`, `version`). It is **not** part of the integrator SDK;
integrating contracts should use `trustline_sdk::ValidationEngineClient`
(`require_*` / `check_*`) instead.

### sanctions-list

Minimal sanctions screening surface (`SanctionsList` contract):

```rust
fn is_sanctioned(addr: Address) -> bool;
```

Reference contract also exposes owner-controlled `set_sanction`.

### trustline-registry

Trustline-controlled registry (`TrustlineRegistry` contract):

| Method | Role |
|--------|------|
| `set_oracle(oracle, approved)` / `is_oracle` | Oracle publishing allowlist |
| `set_record(key, addr)` / `get_record(key)` | Key → address (keccak256 of UTF-8 key) |
| `get_all_records()` | Enumerate live records |
| `remove_record(key)` | Clear a record (key stays indexed; filtered from listing) |

### validation-engine

Injectable facade over `ve-core` (`ValidationEngine` contract) whose `__constructor` takes an explicit `registry` address. Used for unit tests and custom deploys.

### trustline-oracle-ve

Production facade over `ve-core` (`TrustlineOracleVE` contract) with the Trustline registry address baked into the WASM (`VALIDATION_REGISTRY`). Prefer this for production; change the registry by patching `registry_address.rs`, rebuilding, and `upgrade`.

## Implementation Contracts

- **trustline-oracle-ve** — Production Validation Engine with hardcoded registry and instance `upgrade`
- **validation-engine** — Injectable-registry engine for tests and generic deploys
- **sanctions-list** — Owner-controlled sanctions list for testing and development
- **trustline-registry** — Shared oracle allowlist + key → address records

## Contract interface

Signatures below are the Validation Engine's public entrypoints, identical on both facades
(`trustline-oracle-ve` and `validation-engine`). `Dapp` is the only `ValidationMode` supported
today and is what the non-`_adv` helpers pass.

### Enforcing (consume a proof, change state)

| Function | Signature | Auth | Notes |
|---|---|---|---|
| `require_trustline` | `(protocol: Address, sender: Address, value: i128, data: Bytes) -> Result<(), Error>` | `protocol` | Consumes the proof. Use this unless you need sanctions screening on extra addresses. |
| `require_trustline_addrs` | `(protocol, sender, value, data, addresses: Vec<Address>) -> Result<(), Error>` | `protocol` | As above, plus sanctions screening on every address in `addresses`. |
| `require_trustline_adv` | `(protocol, mode: ValidationMode, sender, value, data, addresses) -> Result<(), Error>` | `protocol` | Full form. The other two delegate to this with `mode = Dapp`. |

All three consume the proof on success: it is spendable exactly once. `protocol.require_auth()`
means only the protected contract can spend proofs addressed to it.

### Query (read only, do not consume)

| Function | Signature | Returns |
|---|---|---|
| `check_trustline_status` | `(protocol, sender, value, data) -> bool` | `true` when a spendable proof exists |
| `check_status_addrs` | `(protocol, sender, value, data, addresses) -> bool` | as above, including sanctions screening |
| `check_status_adv` | `(protocol, mode, sender, value, data, addresses) -> bool` | full form |
| `get_tx_state` | `(sender: Address, id: BytesN<32>, addresses: Vec<Address>) -> TxState` | `Approved`, `Rejected`, `Expired`, `Pending` or `Unknown` |
| `compute_intent_id` | `(mode, sender, protocol, value, data) -> BytesN<32>` | the intent id, for off-chain reconciliation |

> Never gate a state change on `check_*`. It reports status without consuming, so a proof
> checked and not consumed can be spent by a later call. Use `require_trustline*` to enforce.

### Oracle and auditor

| Function | Signature | Auth |
|---|---|---|
| `add_tx` | `(oracle: Address, id: BytesN<32>, policy_hash: BytesN<32>, timestamp: u64, approval_required: bool) -> Result<(), Error>` | `oracle`, and must pass `registry.is_oracle` |
| `approve_or_reject_tx` | `(id: BytesN<32>, decision: bool) -> Result<(), Error>` | `auditor` |

`approval_required = false` stores the proof as `Approved` with
`valid_until = timestamp + auto_validity_secs`. `true` stores it as `Pending` for the auditor.

### Administration

| Function | Signature | Auth |
|---|---|---|
| `admin` | `() -> Address` | none |
| `registry` | `() -> Address` | none |
| `version` | `() -> u32` | none |
| `trustline_oracle_enabled` | `() -> bool` | none |
| `sanctions_oracle_enabled` | `() -> bool` | none |
| `transfer_admin` | `(new_admin: Address)` | `admin` |
| `set_auditor` | `(new_auditor: Address)` | `admin` |
| `set_validation_configuration` | `(trustline_enabled: bool, sanctions_enabled: bool, sanctions_key: Option<String>) -> Result<(), Error>` | `admin` |
| `upgrade` | `(new_wasm_hash: BytesN<32>)` | `admin` |

`init` sets the auditor to the admin. A deployment using the manual approval path must call
`set_auditor` to separate the roles. Setting `trustline_enabled = false` disables enforcement
entirely: see [SECURITY.md](SECURITY.md) for why that is fail open.

### trustline-registry

| Function | Signature | Auth |
|---|---|---|
| `set_oracle` | `(oracle: Address, approved: bool)` | `admin` |
| `is_oracle` | `(oracle: Address) -> bool` | none |
| `set_record` | `(key: String, addr: Address) -> Result<(), Error>` | `admin` |
| `get_record` | `(key: String) -> Option<Address>` | none |
| `get_all_records` | `() -> (Vec<String>, Vec<Address>)` | none |
| `remove_record` | `(key: String) -> Result<(), Error>` | `admin` |
| `transfer_admin` | `(new_admin: Address)` | `admin` |
| `upgrade` | `(new_wasm_hash: BytesN<32>)` | `admin` |

## Error codes

Soroban has no revert strings: a failed call surfaces as `Error(Contract, #N)`. These are the
Validation Engine's codes. They reach an integrator's caller through whichever contract
invoked the engine, so a firewall or a protected contract will trap with the engine's code.

| # | Name | Meaning and usual cause |
|---|---|---|
| 1 | `Unauthorized` | The caller is not permitted. For `add_tx` the publisher is not a registry-approved oracle; for owner-only entrypoints the signer is not the owner. |
| 2 | `ValidationExpired` | Returned by `add_tx` only, when the supplied timestamp is already past its validity window. A proof that expires *after* publication is not reported with this code: `check_oracle_state` reports `Expired` and the consuming call fails with code 5. |
| 3 | `ValidationTooEarly` | The proof timestamp is further in the future than `max_skew_secs` allows. |
| 4 | `AlreadySubmitted` | A proof for this intent id already exists with the same timestamp. Proofs are single use: re-run pre-validation for a fresh one. |
| 5 | `NotApproved` | No spendable proof for this exact intent. Pre-validation was skipped, the proof was published for a different sender, amount or arguments than the call being made, or the proof existed and has expired or already been consumed. **This is the code you will see most often.** |
| 6 | `Sanctioned` | The sender or one of the supplied addresses is flagged by the sanctions oracle. |
| 7 | `NotPending` | `approve_or_reject_tx` was called on an intent that is absent or not in the `Pending` state. |
| 8 | `SanctionsKeyUnknown` | `set_validation_configuration` enabled sanctions but the registry key was missing or unresolvable. |

`TxState` values returned by `get_tx_state`: `Approved`, `Rejected`, `Expired`, `Pending`,
`Unknown`. `Unknown` means no entry exists, which is also what an archived or already
consumed proof looks like.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) with target `wasm32v1-none`
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli) (`stellar`)

```bash
rustup target add wasm32v1-none
```

### Installation

```bash
git clone https://github.com/TrustLine-id/stellar-validation-engine.git
cd stellar-validation-engine
```

Add to your contract (this repo depends on **`trustline-sdk`** from crates.io):

```toml
[dependencies]
trustline-sdk = "0.1"
soroban-sdk = "27"
```

For local development alongside an unpublished `trustline-sdk` checkout, see [`.cargo/config.toml.example`](.cargo/config.toml.example).

### Build contracts

```bash
stellar contract build
```

### Test contracts

Run `stellar contract build` first. One upgrade test embeds the compiled WASM with
`include_bytes!`, so `cargo test` fails on a clean checkout until the build has run.

```bash
stellar contract build && cargo test --workspace
```

Run a single package:

```bash
cargo test -p validation-engine --lib
cargo test -p trustline-registry --lib
```

### Deploy (testnet sketch)

Prefer the end-to-end script in the demo app:

[`stellar-demo-app/scripts/deploy-testnet.sh`](../stellar-demo-app/scripts/deploy-testnet.sh)

Flow:

1. Deploy `trustline-registry`, then `set_oracle(backend_oracle, true)` (see demo deploy script)
2. Patch `trustline-oracle-ve/src/registry_address.rs` with the registry id
3. Build & deploy `trustline-oracle-ve`
4. Deploy client contracts pointing at that VE

Manual outline:

```bash
# Registry
stellar contract deploy --wasm target/wasm32v1-none/release/trustline_registry.wasm ... -- --admin <TL>
stellar contract invoke --id <REG> -- set_oracle --oracle <ORACLE> --approved true

# Patch VALIDATION_REGISTRY, rebuild trustline-oracle-ve, then:
stellar contract deploy --wasm-hash <TOVE_HASH> ... -- \
  --admin <ADMIN> \
  --auto-validity-secs 1800 \
  --manual-validity-secs 432000 \
  --max-skew-secs 60
```

Wire a sanctions list (optional):

```bash
# Deploy sanctions-list, then:
stellar contract invoke --id <REG> -- set_record --key "MY_KEY" --addr <SANCTIONS_ID>
stellar contract invoke --id <VE> -- set_validation_configuration \
  --trustline_enabled true --sanctions_enabled true --sanctions_key "MY_KEY"
```

## Development Notes

- **Soroban SDK**: 27.x (Stellar `soroban-sdk` crate)
- **Integrator SDK**: `trustline-sdk = "0.1"` from [crates.io](https://crates.io/crates/trustline-sdk) (repo: [stellar-sdk](https://github.com/TrustLine-id/stellar-sdk))
- **Proof storage**: temporary entries + ledger TTL; applicative `valid_until` remains the source of truth for certificate expiry
- **Upgradeability**: instance `upgrade(new_wasm_hash)` with admin auth
- **Sanctions**: optional; disabled by default until configured via registry key

## Related repositories

| Repo | Role |
|------|------|
| [stellar-validation-engine](https://github.com/TrustLine-id/stellar-validation-engine) | This repo — on-chain VE, registry, sanctions |
| [stellar-sdk](https://github.com/TrustLine-id/stellar-sdk) | Integrator crate `trustline-sdk` on crates.io |
| [stellar-demo-app](https://github.com/TrustLine-id/stellar-demo-app) | React demo |

## License

Copyright (C) 2026 [Trustline Digital Asset Ltd.](https://www.trustline.id). Licensed under the [GNU General Public License v2.0 or later](LICENSE) — see also [NOTICE](NOTICE).

## Contact

- [Trustline](https://www.trustline.id)
- Email: contact@trustline.id
- Issues: https://github.com/TrustLine-id/stellar-validation-engine/issues
