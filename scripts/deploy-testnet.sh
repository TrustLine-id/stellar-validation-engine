#!/usr/bin/env bash
# Deploy Trustline core on-chain infra: TrustlineRegistry (+ oracle allowlist),
# then patch VALIDATION_REGISTRY into trustline-oracle-ve for subsequent VE builds.
#
# Does NOT deploy a Validation Engine instance — build & deploy trustline-oracle-ve
# separately after VALIDATION_REGISTRY is patched (per client).
#
# Env:
#   STELLAR_ACCOUNT   CLI identity (funded) — registry admin (required)
#   STELLAR_NETWORK   default: testnet
#   BACKEND_ORACLE    G… address allowed to add_tx (optional; script default)
#
# Writes:
#   contracts/trustline-oracle-ve/src/registry_address.rs  (patched)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NETWORK="${STELLAR_NETWORK:-testnet}"
SOURCE="${STELLAR_ACCOUNT:?Set STELLAR_ACCOUNT to a funded stellar CLI identity}"

REGISTRY_RS="$ROOT/contracts/trustline-oracle-ve/src/registry_address.rs"

# Backend publisher (add_tx signer).
BACKEND_ORACLE="${BACKEND_ORACLE:-GADELLMHQRWZIYL5YJ264LDTAV3C3I2AQI6TV46WTLUYM3BFG36PDS2Q}"

ADDR="$(stellar keys address "$SOURCE")"
echo "Deployer / registry admin: $ADDR"
echo "Backend oracle (set_oracle): $BACKEND_ORACLE"
echo "Network: $NETWORK"

strip_quotes() {
  tr -d '"'
}

patch_registry_const() {
  local id="$1"
  cat > "$REGISTRY_RS" <<EOF
//! Patched by \`scripts/deploy-testnet.sh\` after TrustlineRegistry deploy.
//! Must be a valid contract strkey (C…) before building the production WASM.

pub const VALIDATION_REGISTRY: &str = "$id";
EOF
}

echo "==> Building TrustlineRegistry"
(cd "$ROOT" && stellar contract build --package trustline-registry >/dev/null)

REG_WASM="$ROOT/target/wasm32v1-none/release/trustline_registry.wasm"

echo "==> Uploading & deploying TrustlineRegistry"
REG_HASH="$(stellar contract upload --wasm "$REG_WASM" --network "$NETWORK" --source-account "$SOURCE" | strip_quotes)"
REG_ID="$(stellar contract deploy \
  --wasm-hash "$REG_HASH" \
  --network "$NETWORK" \
  --source-account "$SOURCE" \
  -- \
  --admin "$ADDR" | strip_quotes)"
echo "REGISTRY=$REG_ID"

echo "==> Authorizing backend oracle"
stellar contract invoke --id "$REG_ID" --network "$NETWORK" --source-account "$SOURCE" -- \
  set_oracle --oracle "$BACKEND_ORACLE" --approved true

echo "==> Patching trustline-oracle-ve VALIDATION_REGISTRY"
patch_registry_const "$REG_ID"

echo ""
echo "Trustline core deployed."
echo "  REGISTRY=$REG_ID"
echo "  Patched: $REGISTRY_RS"
echo ""
echo "Next: build & deploy a client TrustlineOracleVE, e.g.:"
echo "  stellar contract build --package trustline-oracle-ve"
echo "  stellar contract upload --wasm target/wasm32v1-none/release/trustline_oracle_ve.wasm ..."
echo "  stellar contract deploy --wasm-hash <HASH> ... -- --admin <ADMIN> \\"
echo "    --auto-validity-secs 1800 --manual-validity-secs 432000 --max-skew-secs 60"
