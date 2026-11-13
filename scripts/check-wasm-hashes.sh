#!/usr/bin/env bash
# Compare built WASM sha256 digests against the committed wasm-hashes.txt.
#
# Clients instantiate TrustlineOracleVE by WASM hash, so the hash published in
# deployments.json must be reproducible from this repository at a given commit.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXPECTED="$ROOT/wasm-hashes.txt"
DIR="$ROOT/target/wasm32v1-none/release"
failed=0

if [[ ! -f "$EXPECTED" ]]; then
  echo "ERROR: missing $EXPECTED" >&2
  exit 1
fi

while read -r name want; do
  [[ -z "${name:-}" || "$name" == \#* ]] && continue
  path="$DIR/$name"
  if [[ ! -f "$path" ]]; then
    echo "MISSING $name (expected at $path)"
    failed=1
    continue
  fi
  got="$(sha256sum "$path" | awk '{print $1}')"
  if [[ "$got" == "$want" ]]; then
    echo "OK $name"
  else
    echo "MISMATCH $name"
    echo "  expected $want"
    echo "  got      $got"
    failed=1
  fi
done < <(grep -vE '^\s*(#|$)' "$EXPECTED")

if [[ "$failed" -ne 0 ]]; then
  echo >&2
  echo "WASM hashes do not match wasm-hashes.txt." >&2
  echo "If the change is intentional, rebuild without .cargo/config.toml and run:" >&2
  echo "  ./scripts/record-wasm-hashes.sh" >&2
  exit 1
fi

echo "All WASM hashes match wasm-hashes.txt."
