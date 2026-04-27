#!/usr/bin/env bash
set -euo pipefail

# ── WinterWallet Smoke Test ──────────────────────────────────────────
#
# Runs the full wallet lifecycle against surfpool (default) or devnet.
#
# Usage:
#   ./scripts/smoke.sh                                     # surfpool (local)
#   WINTERWALLET_SMOKE_RPC=https://api.devnet.solana.com \
#     WINTERWALLET_SMOKE_KEYPAIR=~/.config/solana/id.json \
#     ./scripts/smoke.sh                                   # devnet
#
# Environment variables:
#   WINTERWALLET_SMOKE_RPC        RPC endpoint (default: http://127.0.0.1:8899)
#   WINTERWALLET_SMOKE_KEYPAIR    Fee-payer keypair JSON (default: ~/.config/solana/id.json)
#   WINTERWALLET_SMOKE_DELAY      Seconds between steps (default: 0 for localhost, 2 for remote)
#   WINTERWALLET_SMOKE_SKIP_BUILD Skip cargo build if set to 1

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SMOKE_RPC="${WINTERWALLET_SMOKE_RPC:-http://127.0.0.1:8899}"
SMOKE_KEYPAIR="${WINTERWALLET_SMOKE_KEYPAIR:-$HOME/.config/solana/id.json}"
SMOKE_SKIP_BUILD="${WINTERWALLET_SMOKE_SKIP_BUILD:-}"

# Auto-detect localhost for delay and surfpool decisions.
is_localhost=false
if [[ "$SMOKE_RPC" == *"127.0.0.1"* || "$SMOKE_RPC" == *"localhost"* ]]; then
  is_localhost=true
fi

SMOKE_DELAY="${WINTERWALLET_SMOKE_DELAY:-}"
if [[ -z "$SMOKE_DELAY" ]]; then
  if $is_localhost; then
    SMOKE_DELAY=0
  else
    SMOKE_DELAY=2
  fi
fi

# Hardcoded test amounts.
FUND_AMOUNT=2           # SOL
WITHDRAW_AMOUNT=1000000  # lamports (must exceed rent-exempt minimum ~890880)
TOKEN_AMOUNT=1000000

BINARY="${repo_root}/target/debug/winterwallet"
SURFPOOL_PID=""
MNEMONIC_FILE=""
RECEIVER_KEYPAIR=""

# ── Colours ──────────────────────────────────────────────────────────

green=""
red=""
cyan=""
reset=""
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
  green=$'\033[32m'
  red=$'\033[31m'
  cyan=$'\033[36m'
  reset=$'\033[0m'
fi

# ── Helpers ──────────────────────────────────────────────────────────

cleanup() {
  if [[ -n "$MNEMONIC_FILE" && -f "$MNEMONIC_FILE" ]]; then
    rm -f "$MNEMONIC_FILE"
  fi
  if [[ -n "$RECEIVER_KEYPAIR" && -f "$RECEIVER_KEYPAIR" ]]; then
    rm -f "$RECEIVER_KEYPAIR"
  fi
  if [[ -n "$SURFPOOL_PID" ]]; then
    kill "$SURFPOOL_PID" 2>/dev/null || true
    wait "$SURFPOOL_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

step_count=0
pass_count=0
fail_count=0

step() {
  step_count=$((step_count + 1))
  printf "\n%s[%d] %s%s\n" "$cyan" "$step_count" "$1" "$reset"
}

pass() {
  pass_count=$((pass_count + 1))
  printf "  %s=> PASS%s\n" "$green" "$reset"
}

fail() {
  fail_count=$((fail_count + 1))
  printf "  %s=> FAIL: %s%s\n" "$red" "$1" "$reset"
  exit 1
}

delay() {
  if [[ "$SMOKE_DELAY" != "0" ]]; then
    sleep "$SMOKE_DELAY"
  fi
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "smoke test requires '$1' but it is not available" >&2
    exit 1
  }
}

# ── Preflight ────────────────────────────────────────────────────────

require_cmd jq
require_cmd solana

if [[ ! -f "$SMOKE_KEYPAIR" ]]; then
  echo "keypair not found: $SMOKE_KEYPAIR" >&2
  echo "Set WINTERWALLET_SMOKE_KEYPAIR or create the default Solana keypair." >&2
  exit 1
fi

has_spl_token=false
if command -v spl-token >/dev/null 2>&1; then
  has_spl_token=true
fi

echo "WinterWallet Smoke Test"
echo "  RPC:     $SMOKE_RPC"
echo "  Keypair: $SMOKE_KEYPAIR"
echo "  Delay:   ${SMOKE_DELAY}s"
echo "  Local:   $is_localhost"

# ── Build ────────────────────────────────────────────────────────────

if [[ -z "$SMOKE_SKIP_BUILD" ]]; then
  step "Building CLI"
  cargo build --manifest-path "${repo_root}/Cargo.toml" --locked
  pass
fi

# ── Surfpool ─────────────────────────────────────────────────────────

if $is_localhost; then
  step "Starting surfpool"
  surfpool start --ci --no-tui --no-studio --no-deploy --features-all &
  SURFPOOL_PID=$!

  # Wait for RPC readiness (max 30s).
  for i in $(seq 1 60); do
    if solana cluster-version --url "$SMOKE_RPC" >/dev/null 2>&1; then
      break
    fi
    if (( i == 60 )); then
      fail "surfpool RPC not ready after 30s"
    fi
    sleep 0.5
  done
  pass
fi

# ── Create ───────────────────────────────────────────────────────────

step "winterwallet create"
MNEMONIC_FILE=$(mktemp)
chmod 600 "$MNEMONIC_FILE"

create_output=$("$BINARY" create --json)
echo "$create_output" | jq -r '.mnemonic' > "$MNEMONIC_FILE"
WALLET_ID=$(echo "$create_output" | jq -r '.wallet_id')
PDA=$(echo "$create_output" | jq -r '.pda')

[[ -n "$WALLET_ID" && "$WALLET_ID" != "null" ]] || fail "missing wallet_id"
[[ -n "$PDA" && "$PDA" != "null" ]] || fail "missing pda"
echo "  Wallet ID: $WALLET_ID"
echo "  PDA:       $PDA"
pass
delay

# ── Init ─────────────────────────────────────────────────────────────

step "winterwallet init"
"$BINARY" init --json --rpc-url "$SMOKE_RPC" --keypair "$SMOKE_KEYPAIR" < "$MNEMONIC_FILE" >/dev/null
pass
delay

# ── Fund PDA ─────────────────────────────────────────────────────────

step "Fund wallet PDA with ${FUND_AMOUNT} SOL"
solana transfer "$PDA" "$FUND_AMOUNT" \
  --url "$SMOKE_RPC" \
  --keypair "$SMOKE_KEYPAIR" \
  --allow-unfunded-recipient \
  --commitment confirmed >/dev/null
pass
delay

# ── Info ─────────────────────────────────────────────────────────────

step "winterwallet info (post-fund)"
info_output=$("$BINARY" info --json --rpc-url "$SMOKE_RPC" < "$MNEMONIC_FILE")
balance=$(echo "$info_output" | jq -r '.on_chain.balance_lamports // empty')
[[ -n "$balance" && "$balance" != "0" ]] || fail "balance is zero or missing after funding"
echo "  Balance: $balance lamports"
pass
delay

# ── Withdraw ─────────────────────────────────────────────────────────

RECEIVER_KEYPAIR=$(mktemp)
solana-keygen new --no-bip39-passphrase -o "$RECEIVER_KEYPAIR" --force >/dev/null 2>&1
RECEIVER=$(solana-keygen pubkey "$RECEIVER_KEYPAIR")

step "winterwallet withdraw ${WITHDRAW_AMOUNT} lamports to ${RECEIVER}"
"$BINARY" withdraw \
  --to "$RECEIVER" \
  --amount "$WITHDRAW_AMOUNT" \
  --json \
  --rpc-url "$SMOKE_RPC" \
  --keypair "$SMOKE_KEYPAIR" < "$MNEMONIC_FILE" >/dev/null
pass
delay

# ── Info (post-withdraw) ─────────────────────────────────────────────

step "winterwallet info (post-withdraw)"
info_output2=$("$BINARY" info --json --rpc-url "$SMOKE_RPC" < "$MNEMONIC_FILE")
balance2=$(echo "$info_output2" | jq -r '.lamports // .balance // empty')
echo "  Balance: $balance2 lamports"
pass
delay

# ── SPL Token Transfer (optional) ────────────────────────────────────

if $has_spl_token; then
  step "SPL token setup (mint + ATAs + mint tokens)"

  # Create a new mint.
  mint_output=$(spl-token create-token --url "$SMOKE_RPC" --fee-payer "$SMOKE_KEYPAIR" 2>&1)
  MINT=$(echo "$mint_output" | grep -oE '[A-Za-z1-9]{32,}' | head -1)
  echo "  Mint: $MINT"

  # Create ATAs.
  spl-token create-account "$MINT" --owner "$PDA" --url "$SMOKE_RPC" --fee-payer "$SMOKE_KEYPAIR" >/dev/null 2>&1
  spl-token create-account "$MINT" --owner "$RECEIVER" --url "$SMOKE_RPC" --fee-payer "$SMOKE_KEYPAIR" >/dev/null 2>&1

  # Mint tokens to PDA's ATA.
  spl-token mint "$MINT" "$TOKEN_AMOUNT" --recipient-owner "$PDA" --url "$SMOKE_RPC" --fee-payer "$SMOKE_KEYPAIR" >/dev/null 2>&1 \
    || spl-token mint "$MINT" "$TOKEN_AMOUNT" "$PDA" --url "$SMOKE_RPC" --fee-payer "$SMOKE_KEYPAIR" >/dev/null 2>&1
  pass
  delay

  step "winterwallet transfer ${TOKEN_AMOUNT} tokens to ${RECEIVER}"
  "$BINARY" transfer \
    --to "$RECEIVER" \
    --mint "$MINT" \
    --amount "$TOKEN_AMOUNT" \
    --json \
    --rpc-url "$SMOKE_RPC" \
    --keypair "$SMOKE_KEYPAIR" < "$MNEMONIC_FILE" >/dev/null
  pass
  delay
else
  echo ""
  echo "  (skipping SPL token transfer - spl-token CLI not available)"
fi

# ── Close ────────────────────────────────────────────────────────────

step "winterwallet close"
"$BINARY" close \
  --to "$RECEIVER" \
  --json \
  --rpc-url "$SMOKE_RPC" \
  --keypair "$SMOKE_KEYPAIR" < "$MNEMONIC_FILE" >/dev/null
pass
delay

# ── Verify Closed ────────────────────────────────────────────────────

step "Verify wallet PDA is closed"
if solana account "$PDA" --url "$SMOKE_RPC" >/dev/null 2>&1; then
  fail "account still exists after close"
fi
pass

# ── Summary ──────────────────────────────────────────────────────────

printf "\n%s========================================%s\n" "$green" "$reset"
printf "%sSMOKE TEST PASSED (%d/%d steps)%s\n" "$green" "$pass_count" "$step_count" "$reset"
printf "%s========================================%s\n" "$green" "$reset"
