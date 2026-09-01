# Security

This document describes the trust model, key custody, replay protection and known
limitations of the Trustline Validation Engine for Stellar / Soroban.

It is written for integrators evaluating whether to protect a contract with Trustline,
and for reviewers assessing the deployed testnet stack. It describes the system **as
implemented today**, not as intended. Where the current implementation is weaker than the
target design, that is stated plainly under [Known limitations](#known-limitations).

Scope: the contracts in this repository (`ve-core`, `validation-engine`,
`trustline-oracle-ve`, `trustline-registry`, `sanctions-list`) and their interaction with
the off-chain Trustline backend. The integration crate and example contracts live in
[stellar-sdk](https://github.com/TrustLine-id/stellar-sdk); the reviewer harness lives in
[stellar-demo-app](https://github.com/TrustLine-id/stellar-demo-app).

---

## Design principle

Decisioning is off-chain, enforcement is on-chain.

The Trustline backend evaluates policy and, on approval, publishes a short-lived proof to
the Validation Engine via `add_tx`. The protected contract calls `require_trustline*`,
which recomputes the intent id from the call it is actually about to perform and consumes
the matching proof. A proof is single use: `consume_validation` removes the storage entry
on success.

The engine never decides anything. It records that a decision was made, binds it to one
specific intent, and lets exactly one call spend it.

---

## Roles and privileges

| Role | Held by | Can do | Authorization |
|---|---|---|---|
| **VE admin** | Deployer key | `transfer_admin`, `set_auditor`, `set_validation_configuration`, `upgrade` | `admin.require_auth()` |
| **Auditor** | Defaults to the admin at `init` | `approve_or_reject_tx` on Pending proofs | `auditor.require_auth()` |
| **Oracle** | Trustline backend publisher key | `add_tx` (publish proofs) | `oracle.require_auth()` **and** `registry.is_oracle(oracle)` |
| **Registry admin** | Deployer key | `set_oracle` (grant or revoke oracle rights), `set_record`, `remove_record`, `transfer_admin`, `upgrade` | `admin.require_auth()` |
| **Protocol contract** | The integrator's contract | Consume its own proofs via `require_trustline*` | `protocol.require_auth()` |
| **Sanctions list owner** | Deployer key | `set_sanction`, `set_owner` | `owner.require_auth()` |

Two properties are worth stating explicitly because they are load-bearing:

- **`init` sets the auditor to the admin.** A production deployment that uses the manual
  approval path must call `set_auditor` to separate the two roles. Until it does, the admin
  key can both publish configuration and adjudicate pending proofs.
- **`require_trustline_adv` calls `protocol.require_auth()`.** Only the protected contract
  itself can consume proofs addressed to it. A third party cannot call the engine directly
  to burn another protocol's proofs, even though intent ids are derivable from public data.

---

## Key custody

**Testnet, today.** A single deployer key holds VE admin, auditor, registry admin and
sanctions list owner. The backend publisher key is a separate account, authorized as an
oracle in the registry. This is a deliberate simplification for the demonstration stack and
is **not** the intended production posture.

**Mainnet, planned.** The production publisher key will be held in managed custody. Admin
and auditor will be separated onto distinct keys, and admin moved behind a multisig. None
of this is in place on the testnet stack, and the specific custody arrangement is out of
scope for this demonstration.

**What a compromised key can do:**

| Compromised | Blast radius |
|---|---|
| Oracle / publisher | Publish arbitrary proofs, so any intent passes policy. Cannot alter engine configuration. Cannot forge a proof for an intent it does not construct correctly. |
| VE admin | Disable enforcement entirely via `set_validation_configuration(trustline_enabled = false)`, replace the auditor, upgrade the WASM. This is the highest-value key. |
| Registry admin | Authorize an arbitrary oracle, which is equivalent to publisher compromise, and take over records. |
| Auditor | Approve or reject Pending proofs. No effect on the auto-approved path. |

---

## Replay protection

Replay protection rests on the intent id and on single-use consumption.

```
intent_id = sha256( XDR( network_id, mode, sender, protocol, value, data ) )
```

`data` is the canonical encoding of the call being made (function name and arguments), so a
proof is bound to one caller, one protected contract, one value and one exact call.

What this gives you:

- **Cross-network replay is prevented.** `network_id` is in the preimage, so a testnet proof
  cannot be replayed on mainnet.
- **Cross-contract replay is prevented.** `protocol` is in the preimage.
- **Cross-caller replay is prevented.** `sender` is in the preimage, and consumption
  additionally requires the protocol contract's own authorization.
- **Argument substitution is prevented.** Changing any argument changes `data` and therefore
  the id. The `wrong_intent_cannot_consume_proof` test asserts three separate near-misses
  cannot spend a proof.
- **Double spend is prevented.** `consume_validation` removes the entry on success, so a
  proof is spendable exactly once.
- **Stale proofs expire.** `valid_until = timestamp + auto_validity_secs`; past that the
  state reads `Expired` and consumption fails with `NotApproved`.
- **Clock skew is bounded.** `add_tx` rejects a timestamp more than `max_skew_secs` in the
  future (`ValidationTooEarly`) or already past its validity window
  (`ValidationExpired`).

Integrators must compute `data` with the same canonical encoding the backend uses. Every
protected method in the SDK exports a pure `*_intent_data` helper for exactly this purpose,
and simulating that helper is today the supported way to obtain the bytes.

---

## Known limitations

None is exploitable by an unprivileged third party; all of them require a
compromised or misbehaving privileged key, or affect only the manual approval path.

1. **`add_tx` replay guard is timestamp-scoped.** The guard rejects a re-add only when
   `existing.created_at == timestamp`. An oracle can therefore re-add the same intent id
   with a different timestamp and overwrite the stored proof, including overwriting a
   `Pending` proof with an `Approved` one and bypassing the auditor. This requires oracle
   rights and is the intended behaviour.

2. **Disabling the oracle is fail open, not fail closed.**
   `set_validation_configuration(trustline_enabled = false)` makes `consume_validation`
   return success with no proof read and `check_*` report `Approved` for everything. There
   is no deny-all or pause mode. During an incident the only configuration lever available
   to the admin weakens enforcement rather than strengthening it. A distinct fail-closed
   `pause()` is considered.

3. **Oracle revocation is not retroactive.** `set_oracle(oracle, false)` stops new
   publications, but proofs already published by that oracle remain valid and spendable
   until they expire. Effective revocation latency is therefore up to `auto_validity_secs`.

4. **Pending proofs use the auto validity budget for storage TTL.** A proof awaiting the
   auditor is stored with `valid_until = 0` and its ledger TTL extended by
   `auto_validity_secs`, not `manual_validity_secs`. On a deployment configured with a short
   auto validity, the entry can be archived well before the intended manual window elapses,
   and a Pending proof that expires reads as `Unknown` rather than `Expired`. The manual
   approval path is currently work-in-progress and is not yet supported by all Trustline
   components.

5. **`policy_hash` is not persisted.** It is emitted in the `TxAdded` event and is not stored
   in `TxMetadata`, so an auditor approving later has no on-chain getter for the policy that
   was evaluated. Reconciliation is possible from event history (`TxPending` / `TxAdded` via
   `final_id`).

6. **The intent preimage does not bind a nonce or an expiry.** Two identical intents from the
   same sender for the same call collapse to one id, so they cannot be distinguished or held
   concurrently. Expiry is a deployment-wide constant applied at registration rather than a
   per-intent value inside the hash.

7. **`encode_call_data` is not injective.** The function name is appended without a length
   prefix, so `("pay", b"_nativeX")` and `("pay_native", b"X")` produce identical bytes. No
   shipped contract is affected, since `pay_native` and `pay_tokens` do not prefix-collide
   and the firewall always uses the literal `forward`. Integrators whose protected methods
   have names where one is a prefix of another should be aware. Length-prefixing is tracked
   as a breaking change for the next SDK minor.

8. **Storage rent is not actively managed.** Registry oracle entries and firewall operator
   entries are bumped on write but not on read, so an entry that is never rewritten can be
   archived after its TTL. A production deployment needs a TTL extension job. See
   [Storage and rent](#storage-and-rent).

---

## Storage and rent

Soroban storage is rented, not bought. Every entry carries a TTL that must be paid to
extend.

- **Proofs** live in temporary storage, are extended at publication and self-evict. This is
  correct and needs no maintenance.
- **Instance state** (admin, registry, auditor, configuration) is bumped on most write paths.
- **Registry oracle entries and records** are bumped when written.

Any long-lived deployment needs a scheduled job that extends the TTL of registry state and
contract instances. This is not implemented in this repository.

This applies to test deployments as well as production ones. A contract instance whose TTL
lapses is archived and its entrypoints stop resolving until the entry is restored, so a
demonstration stack left unattended will stop working. Extend the instances with
`stellar contract extend` and confirm the remaining ledger budget with
`stellar contract info ttl`. In production the registry is managed by Trustline, which
takes care of this.

---

## Sanctions screening

Sanctions screening is **disabled by default** (`init` sets `SanctionsOracleEnabled` to
`false`) and the Trustline oracle is **enabled by default**. That polarity is deliberate:
the absence of configuration must not silently disable enforcement.

When enabled, the sanctions list address is resolved once through the registry at
configuration time and snapshotted. Re-pointing the registry record does not move the
engine to a new list until `set_validation_configuration` is called again.

There is currently no third party publishing a sanctions feed on Stellar. The interface is
implemented and tested against the reference `sanctions-list` contract, but it has not been
exercised against a production feed.

---

## Reporting a vulnerability

Report security issues privately to **security@trustline.id**. Please do not open a public
GitHub issue for a suspected vulnerability.

Include where possible: affected contract and version, the deployed contract id if relevant,
a description of the impact, and reproduction steps or a proof of concept. We aim to
acknowledge within three business days.

The contracts in this repository have **not** undergone an external security audit yet. They
are deployed on testnet for demonstration and evaluation. Do not use them to protect assets
of value on mainnet without an audit.

---

## Related documents

| Document | Scope |
|---|---|
| [README.md](README.md) | Architecture, contract interface, error codes |
| [stellar-sdk](https://github.com/TrustLine-id/stellar-sdk) | Integration crate and example contracts |
| [BACKEND_PREVALIDATION_API.md](https://github.com/TrustLine-id/stellar-demo-app/blob/master/BACKEND_PREVALIDATION_API.md) | Backend pre-validation API and canonical `data` rules |
