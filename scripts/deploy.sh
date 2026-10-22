#!/usr/bin/env bash
#
# Deploy the DepRadar router contract to a Stellar network.
#
# Usage:
#   scripts/deploy.sh [--network <network>] [--source <identity>] [--admin <address>]
#
# Environment variables (used when the corresponding flag is not provided):
#   NETWORK        Stellar network to deploy to (default: testnet)
#   SOURCE         Stellar CLI identity used to sign the deployment (default: depradar-deployer)
#   ADMIN_ADDRESS  Address that will be set as the contract admin (default: address of SOURCE identity)
#   SKIP_OPTIMIZE  Set to "1" to skip `stellar contract optimize`
#
# Examples:
#   scripts/deploy.sh
#   scripts/deploy.sh --network testnet --source alice --admin GABC...
#   NETWORK=futurenet SOURCE=bob scripts/deploy.sh
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
NETWORK="${NETWORK:-testnet}"
SOURCE="${SOURCE:-depradar-deployer}"
ADMIN_ADDRESS="${ADMIN_ADDRESS:-}"
SKIP_OPTIMIZE="${SKIP_OPTIMIZE:-0}"

PACKAGE="deprader_router"
WASM_TARGET="wasm32-unknown-unknown"
WASM_PATH="target/${WASM_TARGET}/release/${PACKAGE}.wasm"
OPTIMIZED_WASM_PATH="target/${WASM_TARGET}/release/${PACKAGE}.optimized.wasm"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log() {
  printf '==> %s\n' "$*" >&2
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command '$1' not found in PATH"
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
  case "$1" in
    --network)
      [[ $# -ge 2 ]] || die "--network requires a value"
      NETWORK="$2"
      shift 2
      ;;
    --source)
      [[ $# -ge 2 ]] || die "--source requires a value"
      SOURCE="$2"
      shift 2
      ;;
    --admin)
      [[ $# -ge 2 ]] || die "--admin requires a value"
      ADMIN_ADDRESS="$2"
      shift 2
      ;;
    --skip-optimize)
      SKIP_OPTIMIZE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1 (use --help for usage)"
      ;;
  esac
done

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------
require_cmd cargo
require_cmd stellar

# Resolve the repository root so the script works from any directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

log "Repository root: ${REPO_ROOT}"
log "Network:         ${NETWORK}"
log "Source identity: ${SOURCE}"

# Verify the source identity exists.
if ! stellar keys address "${SOURCE}" >/dev/null 2>&1; then
  die "stellar identity '${SOURCE}' not found. Create one with: stellar keys generate ${SOURCE} --network ${NETWORK} --fund"
fi

# Default the admin to the deployer's address when not provided.
if [[ -z "${ADMIN_ADDRESS}" ]]; then
  ADMIN_ADDRESS="$(stellar keys address "${SOURCE}")"
  log "Admin address:   ${ADMIN_ADDRESS} (defaulted to source identity)"
else
  log "Admin address:   ${ADMIN_ADDRESS}"
fi

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
log "Building ${PACKAGE} (${WASM_TARGET}, release)..."
cargo build --target "${WASM_TARGET}" --release --package "${PACKAGE}"

[[ -f "${WASM_PATH}" ]] || die "expected wasm artifact not found at ${WASM_PATH}"
log "Built wasm: ${WASM_PATH}"

# ---------------------------------------------------------------------------
# Optimize (optional)
# ---------------------------------------------------------------------------
DEPLOY_WASM="${WASM_PATH}"
if [[ "${SKIP_OPTIMIZE}" != "1" ]]; then
  log "Optimizing wasm..."
  if stellar contract optimize --wasm "${WASM_PATH}" --wasm-out "${OPTIMIZED_WASM_PATH}"; then
    DEPLOY_WASM="${OPTIMIZED_WASM_PATH}"
    log "Optimized wasm: ${DEPLOY_WASM}"
  else
    log "warning: 'stellar contract optimize' failed; deploying unoptimized wasm"
  fi
else
  log "Skipping optimization (SKIP_OPTIMIZE=1)"
fi

# ---------------------------------------------------------------------------
# Deploy
# ---------------------------------------------------------------------------
log "Deploying contract to ${NETWORK}..."
CONTRACT_ID="$(
  stellar contract deploy \
    --wasm "${DEPLOY_WASM}" \
    --network "${NETWORK}" \
    --source "${SOURCE}"
)"

[[ -n "${CONTRACT_ID}" ]] || die "deployment did not return a contract ID"
log "Deployed contract ID: ${CONTRACT_ID}"

# ---------------------------------------------------------------------------
# Initialize
# ---------------------------------------------------------------------------
log "Initializing contract with admin ${ADMIN_ADDRESS}..."
stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source "${SOURCE}" \
  -- \
  initialize \
  --admin "${ADMIN_ADDRESS}"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
printf '\n'
printf 'Deployment complete.\n'
printf '  Network:     %s\n' "${NETWORK}"
printf '  Contract ID: %s\n' "${CONTRACT_ID}"
printf '  Admin:       %s\n' "${ADMIN_ADDRESS}"
printf '\n'
printf 'Next steps:\n'
printf '  - Update the contract ID in README.md, client/ and frontend/ (see DEPLOYMENT.md).\n'
printf '  - Verify with: stellar contract invoke --id %s --network %s --source %s -- get_admin\n' \
  "${CONTRACT_ID}" "${NETWORK}" "${SOURCE}"