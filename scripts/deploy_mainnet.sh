#!/usr/bin/env bash
set -euo pipefail

# Config
RPC_URL="${RPC_URL:-https://mainnet.sorobanrpc.com}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
ADMIN_SECRET="${ADMIN_SECRET:-}"

if [[ -z "${ADMIN_SECRET}" ]]; then
  echo "ERROR: ADMIN_SECRET is empty. Export ADMIN_SECRET before running." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACT_DIR="${ROOT_DIR}/contracts/nft"
WASM_RELEASE="${ROOT_DIR}/target/wasm32v1-none/release/soroban_nft_contract.wasm"
OPT_WASM="${ROOT_DIR}/target/wasm32v1-none/release/soroban_nft_contract.optimized.wasm"

echo "==> Building contract"
cd "${CONTRACT_DIR}"
stellar contract build

echo "==> Optimizing wasm: ${WASM_RELEASE}"
stellar contract optimize --wasm "${WASM_RELEASE}"
echo "Optimized wasm: ${OPT_WASM}"

echo "==> Uploading wasm to mainnet"
UPLOAD_OUT="$(stellar contract upload \
  --wasm "${OPT_WASM}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}" \
  --source-account "${ADMIN_SECRET}" \
  --very-verbose 2>&1 || true)"

echo "${UPLOAD_OUT}" | sed -n '1,150p'

# Try to extract wasm hash from upload output
WASM_HASH="$(printf '%s\n' "${UPLOAD_OUT}" | grep -Eo 'wasm hash: [0-9a-fA-F]+' | awk '{print $3}' | tail -n1)"
if [[ -z "${WASM_HASH}" ]]; then
  echo "ERROR: Could not parse wasm hash from upload output." >&2
  exit 2
fi
echo "WASM_HASH=${WASM_HASH}"

echo "==> Deploying (create_contract)"
SALT="$(openssl rand -hex 32)"
DEPLOY_OUT="$(stellar contract deploy \
  --wasm-hash "${WASM_HASH}" \
  --salt "${SALT}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}" \
  --source-account "${ADMIN_SECRET}" 2>&1 || true)"

echo "${DEPLOY_OUT}" | sed -n '1,150p'

# Extract Contract ID
CONTRACT_ID="$(printf '%s\n' "${DEPLOY_OUT}" | grep -Eo 'Contract ID: [A-Z0-9]+' | awk '{print $3}' | tail -n1)"
if [[ -z "${CONTRACT_ID}" ]]; then
  echo "ERROR: Could not parse Contract ID from deploy output." >&2
  exit 3
fi

echo
echo "MYLAB_CONTRACT_ID=${CONTRACT_ID}"
echo "Done."


