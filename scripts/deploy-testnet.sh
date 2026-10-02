#!/usr/bin/env bash
# Deploy Trustline core on-chain infra:
#   1) TrustlineRegistry (+ oracle allowlist)
#   2) Patch VALIDATION_REGISTRY, build & upload TrustlineOracleVE WASM (no deploy)
#   3) Append an entry to deployments.json (committed reference for clients)
#
# Does NOT deploy a Validation Engine instance — clients deploy with VE_WASM_HASH.
#
# Env:
#   STELLAR_ACCOUNT   CLI identity (funded) — registry admin (required)
#   STELLAR_NETWORK   default: testnet
#   BACKEND_ORACLE    G… address allowed to add_tx (optional; script default)
#
# Writes (commit & push):
#   contracts/trustline-oracle-ve/src/registry_address.rs  (patched VALIDATION_REGISTRY)
#   deployments.json                                       (append ve_uploads entry)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NETWORK="${STELLAR_NETWORK:-testnet}"
SOURCE="${STELLAR_ACCOUNT:?Set STELLAR_ACCOUNT to a funded stellar CLI identity}"

REGISTRY_RS="$ROOT/contracts/trustline-oracle-ve/src/registry_address.rs"
DEPLOYMENTS_JSON="$ROOT/deployments.json"

# Backend publisher (add_tx signer).
BACKEND_ORACLE="${BACKEND_ORACLE:-GADELLMHQRWZIYL5YJ264LDTAV3C3I2AQI6TV46WTLUYM3BFG36PDS2Q}"

# Trustline backend chain ids: 1=mainnet, 2=testnet, 3=futurenet
case "$NETWORK" in
  mainnet) CHAIN_ID=1 ;;
  testnet) CHAIN_ID=2 ;;
  futurenet) CHAIN_ID=3 ;;
  *)
    echo "ERROR: unsupported STELLAR_NETWORK='$NETWORK' (expected mainnet|testnet|futurenet)" >&2
    exit 1
    ;;
esac

ADDR="$(stellar keys address "$SOURCE")"
echo "Deployer / registry admin: $ADDR"
echo "Backend oracle (set_oracle): $BACKEND_ORACLE"
echo "Network: $NETWORK (chain_id=$CHAIN_ID)"

strip_quotes() {
  tr -d '"'
}

patch_registry_const() {
  local id="$1"
  cat > "$REGISTRY_RS" <<EOF
//! Patched by \`scripts/deploy-testnet.sh\` after TrustlineRegistry deploy.
//! Commit this file with the live registry strkey (C…) used by uploaded VE WASMs.

pub const VALIDATION_REGISTRY: &str = "$id";
EOF
}

# On-chain contract version() constants (baked into WASM / deployed registry).
read_src_version() {
  local file="$1"
  python3 - "$file" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r"pub fn version\([^)]*\)\s*->\s*u32\s*\{\s*(\d+)", text)
if not m:
    sys.exit(f"could not parse version() in {sys.argv[1]}")
print(m.group(1))
PY
}

append_ve_upload() {
  python3 - "$DEPLOYMENTS_JSON" "$NETWORK" "$CHAIN_ID" "$1" "$2" "$3" "$4" <<'PY'
import json, sys
path, network, chain_id, wasm_hash, ve_ver, reg_id, reg_ver = sys.argv[1:8]
try:
    data = json.load(open(path, encoding="utf-8"))
except FileNotFoundError:
    data = {}
net = data.setdefault(network, {})
uploads = net.setdefault("ve_uploads", [])
uploads.append({
    "wasm_hash": wasm_hash,
    "version": int(ve_ver),
    "chain_id": int(chain_id),
    "registry_contract_id": reg_id,
    "registry_version": int(reg_ver),
})
json.dump(data, open(path, "w", encoding="utf-8"), indent=2)
open(path, "a", encoding="utf-8").write("\n")
PY
}

VE_VERSION="$(read_src_version "$ROOT/contracts/ve-core/src/lib.rs")"
REG_VERSION_SRC="$(read_src_version "$ROOT/contracts/trustline-registry/src/lib.rs")"

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

REG_VERSION="$(stellar contract invoke --id "$REG_ID" --network "$NETWORK" --source-account "$SOURCE" -- version | strip_quotes)"
# Prefer on-chain value; fall back to source if invoke output is unexpected.
if [[ ! "$REG_VERSION" =~ ^[0-9]+$ ]]; then
  REG_VERSION="$REG_VERSION_SRC"
fi
echo "REGISTRY_VERSION=$REG_VERSION"

echo "==> Patching trustline-oracle-ve VALIDATION_REGISTRY"
patch_registry_const "$REG_ID"

echo "==> Building & uploading TrustlineOracleVE (no deploy)"
(cd "$ROOT" && stellar contract build --package trustline-oracle-ve >/dev/null)
VE_WASM="$ROOT/target/wasm32v1-none/release/trustline_oracle_ve.wasm"
VE_WASM_HASH="$(stellar contract upload --wasm "$VE_WASM" --network "$NETWORK" --source-account "$SOURCE" | strip_quotes)"
echo "VE_WASM_HASH=$VE_WASM_HASH"
echo "VE_VERSION=$VE_VERSION"

echo "==> Updating deployments.json"
append_ve_upload "$VE_WASM_HASH" "$VE_VERSION" "$REG_ID" "$REG_VERSION"

echo ""
echo "Trustline core ready (registry deployed, VE WASM uploaded — not instantiated)."
echo "  REGISTRY=$REG_ID (v$REG_VERSION)"
echo "  VE_WASM_HASH=$VE_WASM_HASH (v$VE_VERSION)"
echo "  Patched:  $REGISTRY_RS"
echo "  Recorded: $DEPLOYMENTS_JSON"
echo ""
echo "Client deploy (no VE checkout needed):"
echo "  export VE_WASM_HASH=$VE_WASM_HASH"
echo "  export REGISTRY_ID=$REG_ID"
echo "  # then run the client repo deploy script"
echo ""
echo "Commit & push registry_address.rs + deployments.json when you want this release in git."
