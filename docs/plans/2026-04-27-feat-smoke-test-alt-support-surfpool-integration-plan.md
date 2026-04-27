---
title: "feat: CLI Smoke Test, ALT Support, and Surfpool Integration Tests"
type: feat
date: 2026-04-27
---

# CLI Smoke Test, ALT Support, and Surfpool Integration Tests

## Enhancement Summary

**Deepened on:** 2026-04-27
**Research agents used:** TypeScript reviewer, Security sentinel, Performance oracle, Architecture strategist, Best-practices researcher, Framework-docs researcher, Code-simplicity reviewer, Pattern-recognition specialist

### Key Improvements from Research
1. **Security blocker found**: CLI's `rpassword::prompt_password` reads `/dev/tty`, not stdin — smoke test will hang without a CLI code change
2. **API surface reduced**: dropped `withComputeBudgetV0` and `AdvancePlan.estimateV0TransactionSize` method — existing `withComputeBudget` is version-agnostic
3. **Fixture security**: removed mnemonic from signing-session fixture, fixed pseudocode that used undefined `digest` variable
4. **Performance**: default `SMOKE_DELAY=0` for localhost saves ~20s, parallel builds save ~5-15s
5. **Wire format grounded**: exact v0 message layout confirmed from `@solana/web3.js` v1.98.4 type declarations

### New Considerations Discovered
- ALT account substitution is mitigated by the Advance preimage binding raw addresses — document as security invariant
- Breakeven for ALT savings: 2+ non-signer accounts per ALT table (~34 byte overhead per table, 31 bytes saved per resolved account)
- Pin `surfpool-sdk` to exact version (pre-1.0, caret ranges are dangerous)
- Add CI step to detect stale fixtures via `git diff --exit-code fixtures/`

---

## Overview

Three workstreams to harden winterwallet now that the program is deployed on devnet and mainnet:

1. **CLI smoke test script** — bash script running the full wallet lifecycle against surfpool (default) or devnet
2. **ALT support in TypeScript SDK** — versioned transaction (v0) support with Address Lookup Tables
3. **Surfpool TypeScript integration tests** — real on-chain transactions via the TS SDK against embedded surfpool

## Why This Approach

The program is live. We need confidence that the CLI, the TS SDK, and the on-chain program work end-to-end — not just at the unit/fixture level. The existing golden-vector tests guarantee encoding correctness across Rust and TypeScript, but they don't touch a real validator.

ALT support is needed because Winternitz signatures consume 768 bytes of instruction data, leaving only ~430 bytes in a legacy transaction for everything else. With 6+ account keys at 32 bytes each, complex operations (token transfers, multi-CPI advances) are at the edge of what fits. ALTs compress account keys to 1-byte indices, meaningfully expanding capacity.

## Key Decisions

- **Surfpool over solana-test-validator**: Faster boot, embedded Node SDK, cheatcodes for account injection
- **Signing session fixtures**: TS SDK has no Winternitz signing — extend `client/tests/regen.rs` to generate a signing-session fixture with real signatures for the TS integration tests
- **ALTs are transaction-level only**: The Advance preimage digest includes raw addresses. Solana resolves ALT indices before the program sees accounts. No on-chain changes needed.
- **Legacy remains the default**: All existing APIs stay unchanged. ALT/v0 support is additive.

---

## Phase 0: CLI Prerequisite — stdin Mnemonic Fallback

### What

The CLI's `read_mnemonic()` at `cli/src/helpers.rs:37-43` uses `rpassword::prompt_password()`, which reads from `/dev/tty`, not stdin. When stdin is a pipe, `rpassword` opens `/dev/tty` directly — the pipe is ignored and the command **hangs forever**. This blocks the entire smoke test.

### Fix

Modify `read_mnemonic()` to detect when stdin is not a TTY and fall back to reading from stdin:

```rust
pub fn read_mnemonic() -> Result<Zeroizing<String>, String> {
    use std::io::IsTerminal;
    let raw = if std::io::stdin().is_terminal() {
        Zeroizing::new(
            rpassword::prompt_password("Enter mnemonic: ")
                .map_err(|e| format!("failed to read mnemonic: {e}"))?,
        )
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_line(&mut buf)
            .map_err(|e| format!("failed to read mnemonic from stdin: {e}"))?;
        Zeroizing::new(buf)
    };
    Ok(Zeroizing::new(raw.trim().to_string()))
}
```

### Research Insights

**Security consideration**: `std::io::IsTerminal` (stabilized in Rust 1.70) is the correct way to detect TTY. Do not use the `atty` crate — it is unmaintained and has a Windows-specific soundness issue.

### Files to Modify

| File | Changes |
|------|---------|
| `cli/src/helpers.rs` | Add stdin fallback to `read_mnemonic()` |
| `cli/Cargo.toml` | No new dependencies needed (`IsTerminal` is in std) |

### Acceptance Criteria

- [x] `echo "test mnemonic words" | winterwallet info --json` reads from stdin instead of hanging
- [x] Interactive mode still prompts via `rpassword` when run from a terminal
- [x] Mnemonic is still `Zeroize`-wrapped in both paths

---

## Phase 1: CLI Smoke Test Script

### What

A self-contained bash script at `scripts/smoke.sh` that:
1. Optionally starts surfpool with the winterwallet program
2. Runs the full wallet lifecycle via CLI commands
3. Validates JSON output and on-chain state at each step
4. Supports devnet override via env var

### Flow

```
[start surfpool (if local)] → create → init → fund PDA → info
  → withdraw → info → transfer (SPL token) → info → close → verify closed
```

### Files to Create

#### `scripts/smoke.sh`

**Setup section:**
```bash
#!/usr/bin/env bash
set -euo pipefail

# ── Configuration (4 env vars only) ──────────────────────
SMOKE_RPC="${WINTERWALLET_SMOKE_RPC:-http://127.0.0.1:8899}"
SMOKE_KEYPAIR="${WINTERWALLET_SMOKE_KEYPAIR:-$HOME/.config/solana/id.json}"
SMOKE_DELAY="${WINTERWALLET_SMOKE_DELAY:-0}"
SMOKE_SKIP_BUILD="${WINTERWALLET_SMOKE_SKIP_BUILD:-}"

# Hardcoded test amounts (not configurable — this is a smoke test)
FUND_AMOUNT=2          # SOL
WITHDRAW_AMOUNT=100000 # lamports
TOKEN_AMOUNT=1000000

BINARY="./target/debug/winterwallet"
SURFPOOL_PID=""
MNEMONIC_FILE=""
```

### Research Insights

**Delay handling (Performance reviewer):**
Default `SMOKE_DELAY=0` for localhost. Surfpool confirms transactions synchronously — the CLI already waits for confirmation before returning. Delays are only needed for devnet where confirmation is asynchronous. Auto-detect:
```bash
if [[ "$SMOKE_RPC" == *"127.0.0.1"* || "$SMOKE_RPC" == *"localhost"* ]]; then
  SMOKE_DELAY=0
fi
```
**Impact: Saves ~20 seconds per run (largest single optimization).**

**Surfpool lifecycle:**
- Derive surfpool skip from RPC URL — no separate env var needed
- Start surfpool in CI mode: `surfpool start --ci --no-tui --no-studio --artifacts-path ./target/deploy &`
- Wait for RPC readiness: poll `solana cluster-version --url $SMOKE_RPC` in a loop (max 30s)
- Trap handler to kill surfpool on script exit: `trap cleanup EXIT`
- Skip surfpool if RPC is not localhost

**Build step (Performance reviewer):**
Parallelize CLI and SBF builds — they target different directories (`target/debug/` vs `target/deploy/`):
```bash
if [[ -z "$SMOKE_SKIP_BUILD" ]]; then
  cargo build --manifest-path ./Cargo.toml --locked &
  PID_CLI=$!
  cargo build-sbf --manifest-path ./program/Cargo.toml &
  PID_SBF=$!
  wait $PID_CLI $PID_SBF
fi
```
**Impact: Saves ~5-15 seconds on warm cache.**

**Mnemonic handling (Security reviewer):**
Do NOT use shell variables for the mnemonic. Use a temp file with restricted permissions:
```bash
MNEMONIC_FILE=$(mktemp)
chmod 600 "$MNEMONIC_FILE"
trap 'rm -f "$MNEMONIC_FILE"; cleanup' EXIT

$BINARY create --json | jq -r '.mnemonic' > "$MNEMONIC_FILE"
WALLET_ID=$($BINARY create --json | jq -r '.wallet_id')  # separate call for other fields
PDA=$($BINARY create --json | jq -r '.pda')

# Pipe mnemonic to commands via file, not echo:
$BINARY init --json --rpc-url "$SMOKE_RPC" --keypair "$SMOKE_KEYPAIR" < "$MNEMONIC_FILE"
```

Use `printf '%s\n'` if piping is needed instead of `echo` — `echo` may spawn a child process on some systems, exposing the mnemonic via `/proc/PID/cmdline`.

For GitHub Actions CI, add `::add-mask::` to redact the mnemonic from logs.

**Test steps (each with assertions):**

| Step | Command | Assertions |
|------|---------|------------|
| create | `$BINARY create --json` | JSON has `mnemonic`, `wallet_id`, `pda` |
| init | `$BINARY init --json --rpc-url ... --keypair ... < $MNEMONIC_FILE` | Exit 0, JSON success |
| fund | `solana transfer $PDA $FUND_AMOUNT --url $SMOKE_RPC --keypair $SMOKE_KEYPAIR --allow-unfunded-recipient` | Exit 0 |
| info | `$BINARY info --json --rpc-url ... < $MNEMONIC_FILE` | `balance > 0`, `root` present |
| withdraw | `$BINARY withdraw --to $RECEIVER --amount $WITHDRAW_AMOUNT --json ... < $MNEMONIC_FILE` | Exit 0 |
| info (post) | Same as above | Balance decreased |
| token setup | `spl-token create-token ...`, `spl-token create-account ...`, `spl-token mint ...` | ATAs exist |
| transfer | `$BINARY transfer --to $RECEIVER --mint $MINT --amount $TOKEN_AMOUNT --json ... < $MNEMONIC_FILE` | Exit 0 |
| close | `$BINARY close --to $RECEIVER --json ... < $MNEMONIC_FILE` | Exit 0 |
| verify closed | `solana account $PDA --url $SMOKE_RPC` | Account not found |

**Token setup details:**
- Create a new mint: `spl-token create-token --url $SMOKE_RPC --fee-payer $SMOKE_KEYPAIR`
- Create ATAs in parallel (Performance reviewer):
  ```bash
  spl-token create-account $MINT --owner $PDA ... &
  spl-token create-account $MINT --owner $RECEIVER ... &
  wait
  ```
- Mint tokens to PDA's ATA: `spl-token mint $MINT $TOKEN_AMOUNT $PDA_ATA ...`
- Token transfer step skipped gracefully if `spl-token` is not available

**Error handling:**
- Each step wrapped in a helper that prints step name, runs command, checks exit code
- On failure: print full command output, exit with non-zero
- Global timeout: 5min for surfpool, 10min for devnet

**Expected runtime (Performance reviewer):**

| Mode | Estimated |
|------|-----------|
| Warm cache, surfpool | ~15 seconds |
| Cold cache, surfpool | ~55 seconds |
| Devnet (warm cache) | ~45 seconds |

### Acceptance Criteria

- [x] `scripts/smoke.sh` runs the full lifecycle against surfpool with no manual intervention
- [x] `WINTERWALLET_SMOKE_RPC=https://api.devnet.solana.com ./scripts/smoke.sh` works against devnet
- [x] Each step validates JSON output and/or on-chain state
- [x] Surfpool is started/stopped automatically when targeting localhost
- [x] Token transfer step is skipped gracefully if `spl-token` is not available
- [x] Script exits non-zero on any step failure with clear error message
- [x] Mnemonic is stored in a temp file with `chmod 600`, not a shell variable

---

## Phase 2: ALT Support in TypeScript SDK

### What

Add versioned transaction (v0) support to the TS SDK so consumers can use Address Lookup Tables to fit more complex operations in a single Advance transaction.

### Why ALTs Matter Here

| Metric | Legacy | V0 + ALT (5 accounts resolved) |
|--------|--------|-------------------------------|
| Per-account cost in message | 32 bytes | 1 byte (index) |
| ALT overhead per table | 0 | ~34 bytes (32-byte address + compact-u16 lengths) |
| Net savings (5 accounts) | 0 | ~121 bytes |
| Available post-signature budget | ~430 bytes | ~551 bytes |
| Breakeven | — | 2 accounts per ALT |

Common ALT candidates for winterwallet: winterwallet program ID, system program, compute budget program, SPL token program, token-2022 program — all static, never change.

### Design

**Core principle**: ALTs are a transaction-level concern. All instruction builders are unchanged. New functions are additive and follow existing patterns.

### Research Insights

**V0 wire format (Framework docs, confirmed from `@solana/web3.js` v1.98.4 type declarations):**
```
1 byte                              // version prefix (0x80)
3 bytes                             // message header (numSigners, numReadonlySigned, numReadonlyUnsigned)
compactU16(staticKeys.length)       // only keys NOT resolved via ALT
staticKeys.length * 32              // static key bytes
32 bytes                            // recent blockhash
compactU16(instructions.length)
per instruction:
  1 byte                            // program ID index (into combined static+lookup key list)
  compactU16(keys.length)
  keys.length bytes                 // account indices (1 byte each)
  compactU16(data.length)
  data.length bytes
compactU16(addressTableLookups.length)
per ALT:
  32 bytes                          // ALT on-chain address
  compactU16(writableIndexes.length) + writableIndexes.length
  compactU16(readonlyIndexes.length) + readonlyIndexes.length

+ compactU16(requiredSignatures) + requiredSignatures * 64  // signatures
```

**ALT partitioning rules (from `CompiledKeys.extractTableLookup()` in web3.js):**
- Payer is always in static keys (it's a signer)
- **Any signer must be in static keys** — ALTs cannot provide signer status
- **Invoked programs (programId) must be in static keys** — ALTs cannot provide invoked status
- Writable non-signer, non-program accounts → ALT `writableIndexes`
- Readonly non-signer, non-program accounts → ALT `readonlyIndexes`
- Accounts not found in any ALT → static keys
- **Duplicate across ALTs**: only first match is used (web3.js drains matched keys from the map)

**Wallet PDA is eligible for ALT lookup** — it appears as a non-signer, writable account in Advance instructions (signer flags are scrubbed). This is an important space saving.

**Security invariant (Security reviewer):** ALT account substitution is mitigated by the Advance preimage binding raw 32-byte addresses. The on-chain handler at `program/src/instructions/advance.rs:127-129` hashes the actual resolved addresses into the preimage, so a malicious ALT substitution causes signature verification to fail, not silent wrong execution. **Document this as a comment in the ALT code.**

#### New Functions in `transaction.ts`

```typescript
import { AddressLookupTableAccount } from "@solana/web3.js";

/**
 * Estimate the wire size of a v0 transaction with address lookup tables.
 *
 * Accounts found in the provided ALTs are referenced by 1-byte index
 * instead of 32-byte pubkey, reducing message size. Signers and invoked
 * programs always remain in static keys per Solana v0 spec.
 *
 * Security note: ALT account substitution is safe because the Advance
 * preimage binds raw account addresses into the Winternitz digest.
 * Do not remove address binding from the preimage.
 */
export function estimateV0TransactionSize(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[]
): number;

/**
 * Throws TRANSACTION_TOO_LARGE if the estimated v0 size exceeds the limit.
 */
export function assertV0TransactionSize(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[],
  limit?: number
): number;
```

**Internal helper (NOT exported — matches `collectLegacyAccounts` pattern):**

```typescript
/**
 * Partition accounts into static keys and ALT lookups.
 * Uses a Map<string, ...> keyed on base58 for O(1) ALT membership checks.
 */
function partitionV0Accounts(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[]
): { /* internal shape for size estimation */ }
```

**Implementation detail (Performance reviewer):** Pre-build a `Map<string, { tableIndex: number, entryIndex: number }>` keyed on `pubkey.toBase58()` from all ALT entries before iterating instruction accounts. This gives O(1) lookup per account instead of O(T*E) linear scan. With typical numbers (12 accounts, 2 ALTs, 256 entries each), this reduces from ~6,144 comparisons to ~12 map lookups.

#### What Does NOT Change

- `createAdvanceInstruction` — instruction data is identical for legacy and v0
- `encodeAdvancePayload` — payload encoding is transaction-format agnostic
- `advanceDigest` / `initializeDigest` — preimage uses raw addresses, not ALT indices
- `createInitializeInstruction`, `createWithdrawInstruction`, `createCloseInstruction`
- `estimateLegacyTransactionSize`, `assertLegacyTransactionSize` — legacy path preserved
- `withComputeBudget` — already version-agnostic (just prepends instructions)

#### What Was Removed from the Original Plan

Per TS reviewer, simplicity reviewer, and architecture reviewer consensus:

| Removed | Reason |
|---------|--------|
| `withComputeBudgetV0` method on `AdvancePlan` | `withComputeBudget()` is version-agnostic — it just prepends instructions. ALTs are orthogonal. Consumers pass ALTs separately. |
| `AdvancePlan.estimateV0TransactionSize` method | Thin wrapper. Consumers call `estimateV0TransactionSize(payer, plan.withComputeBudget(sig), alts)` directly. |
| Export of `partitionV0Accounts` | Internal helper, matches `collectLegacyAccounts` being private. |
| Named return type for `partitionV0Accounts` | Inline the type. Only one caller. |
| Selective re-exports in `index.ts` | Existing `export * from "./transaction.js"` barrel picks up new exports automatically. |

#### Consumer Usage Pattern

```typescript
import {
  createWithdrawPlan,
  withComputeBudget,
  estimateV0TransactionSize,
} from "@blueshift-gg/winterwallet";
import { MessageV0, VersionedTransaction, AddressLookupTableAccount } from "@solana/web3.js";

// Build plan (unchanged)
const plan = createWithdrawPlan({ walletPda, receiver, lamports, newRoot });
const digest = await plan.digest(walletId, currentRoot);
// ... sign digest with Winternitz keypair ...

// Build instructions with compute budget (unchanged — version-agnostic)
const instructions = plan.withComputeBudget(signatureBytes);

// Estimate v0 size (new)
const size = estimateV0TransactionSize(payer, instructions, [lookupTable]);

// Build v0 transaction (consumer's job — uses @solana/web3.js directly)
const messageV0 = MessageV0.compile({
  payerKey: payer,
  recentBlockhash: blockhash,
  instructions,
  addressLookupTableAccounts: [lookupTable],
});
const tx = new VersionedTransaction(messageV0);
tx.sign([payerKeypair]);
```

#### Optional: `COMMON_ALT_CANDIDATES` constant

Per architecture reviewer, consider exporting a constant listing static program addresses that are always present in winterwallet transactions. This helps consumers build their ALTs:

```typescript
/** Static program addresses commonly present in WinterWallet transactions.
 *  Consumers can include these in their Address Lookup Table for space savings. */
export const COMMON_ALT_CANDIDATES: readonly PublicKey[] = [
  WINTERWALLET_PROGRAM_ID,
  COMPUTE_BUDGET_PROGRAM_ID,
  SystemProgram.programId,
] as const;
```

#### Mock ALT for Testing

From framework docs research — constructing test ALTs without network:

```typescript
const mockLookupTable = new AddressLookupTableAccount({
  key: new PublicKey("..."),
  state: {
    deactivationSlot: BigInt("18446744073709551615"), // U64_MAX = active
    lastExtendedSlot: 0,
    lastExtendedSlotStartIndex: 0,
    authority: undefined,
    addresses: [programId, systemProgram, computeBudgetProgram],
  },
});
```

### Files to Modify

| File | Changes |
|------|---------|
| `sdk/ts/src/transaction.ts` | Add private `partitionV0Accounts`, exported `estimateV0TransactionSize`, `assertV0TransactionSize` |
| `sdk/ts/src/constants.ts` | Optionally add `COMMON_ALT_CANDIDATES` |
| `sdk/ts/test/index.test.ts` | Add v0 size estimation tests |

Note: `index.ts` needs NO changes — existing `export * from "./transaction.js"` picks up new exports.

### Test Cases

```typescript
describe("v0 transaction support", () => {
  it("estimateV0TransactionSize returns smaller size than legacy when ALT resolves accounts");
  it("estimateV0TransactionSize with empty ALT array is slightly larger than legacy (v0 overhead)");
  it("signer accounts are never placed in ALT lookups");
  it("invoked programs are never placed in ALT lookups");
  it("writable non-signer accounts go to ALT writableIndexes");
  it("readonly non-signer accounts go to ALT readonlyIndexes");
  it("accounts not in any ALT go to static keys");
  it("assertV0TransactionSize throws TRANSACTION_TOO_LARGE for oversized transactions");
  // Verification test: build a real VersionedTransaction and compare .serialize().length
  it("estimation matches actual VersionedTransaction serialization size");
});
```

### Acceptance Criteria

- [x] `estimateV0TransactionSize` correctly accounts for static keys vs ALT lookups
- [x] Signer and invoked-program accounts are never placed in ALT lookup sections
- [x] Size estimation matches actual `VersionedTransaction.serialize().length` (verified in tests)
- [x] Reuses existing `TRANSACTION_TOO_LARGE` error code
- [x] All existing tests continue to pass (no breaking changes)
- [x] Uses `readonly` array types in all parameter positions
- [x] Security invariant comment documents preimage address binding

---

## Phase 3: Surfpool TypeScript Integration Tests

### What

A new test file `sdk/ts/test/integration.test.ts` that sends real transactions to an embedded surfpool instance via the `surfpool-sdk` npm package.

### Pre-requisite: Signing Session Fixture

The TS SDK has no Winternitz signing capability — that lives in `winterwallet-core` (Rust, `no_std`). Integration tests need pre-generated valid signatures.

### Research Insights

**Why fixtures, not WASM (Architecture reviewer):**
- `winterwallet-core` depends on `hmac`, `sha2`, `pbkdf2`, `zeroize` — compiling to WASM would require verifying all work under `wasm32-unknown-unknown`, adding `wasm-bindgen`, and creating a JS wrapper
- The fixture approach keeps coupling at the data level (JSON), which is easier to version and debug
- The existing cross-language fixture pattern (`regen.rs` → `fixtures/*.json`) is well-established

**Fixture format (Security + Simplicity reviewers):**

Do NOT include:
- `mnemonic` — security risk, not needed by TS tests
- `type`, `receiver`, `lamports` per session — TS test builds its own instructions

DO include (mirror existing golden vector structure):
- Per-session: `current_root`, `new_root`, `signature`, `digest`, `payload`, `passthrough_accounts`
- Top-level: `wallet_id`, `wallet_pda`, `wallet_bump`, `payer`, `receiver`

```json
{
  "name": "signing_session_v1",
  "wallet_id": "...",
  "wallet_pda": "...",
  "wallet_bump": 255,
  "payer": "...",
  "receiver": "...",
  "sessions": [
    {
      "position": 0,
      "current_root": "...",
      "new_root": "...",
      "signature": "...",
      "digest": "..."
    },
    {
      "position": 1,
      "current_root": "...",
      "new_root": "...",
      "signature": "...",
      "digest": "...",
      "lamports": "500000",
      "payload": "...",
      "passthrough_accounts": [...]
    },
    {
      "position": 2,
      "current_root": "...",
      "new_root": "...",
      "signature": "...",
      "digest": "...",
      "payload": "...",
      "passthrough_accounts": [...]
    }
  ]
}
```

**Extend `client/tests/regen.rs`** — each session must build the actual instruction, compute the preimage, then sign (the original plan's pseudocode used an undefined `digest` variable):

```rust
#[test]
#[ignore]
fn regen_signing_session() {
    // WARNING: This mnemonic is a well-known test vector.
    // NEVER fund the derived wallet on mainnet.
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let mut keypair = WinternitzKeypair::from_mnemonic(mnemonic, 0).unwrap();

    let wallet_id = wallet_id_from_mnemonic(mnemonic).unwrap();
    let (wallet_pda, bump) = find_wallet_address(&wallet_id);

    // Session 0: Initialize
    let init_root = keypair.root();
    let init_preimage = initialize_preimage(&wallet_id);
    let init_digest = solana_sha256_hasher::hashv(&init_preimage).to_bytes();
    let init_sig = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&init_digest);
    let next_root = keypair.root();

    // Session 1: Advance(Withdraw)
    let current_root_1 = next_root;
    let plan_1 = AdvancePlan::withdraw(&wallet_pda, &receiver, lamports, &keypair.root()).unwrap();
    let preimage_1 = plan_1.preimage(&wallet_id, &current_root_1);
    let digest_1 = solana_sha256_hasher::hashv(&preimage_1).to_bytes();
    let sig_1 = keypair.sign_and_increment::<WINTERNITZ_SCALARS>(&digest_1);
    let next_root_1 = keypair.root();

    // Session 2: Advance(Close) — similar pattern
    // ...
}
```

**Performance (Performance reviewer):** Signing session generation is ~37K SHA-256 calls = 3-15ms. No need to cache.

**Security (Security reviewer):**
- Consider adding `fixtures/signing-session.json` to `.gitignore` so real signatures are never committed. Generate locally and in CI via `cargo test --ignored`.
- Alternatively, commit the fixture (like existing golden vectors) but add a warning comment.

### Test Structure

#### `sdk/ts/test/integration.test.ts`

```typescript
import { Surfnet } from "surfpool-sdk";
import { Connection, Keypair, PublicKey, Transaction, sendAndConfirmTransaction } from "@solana/web3.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";

const session = JSON.parse(readFileSync("../../fixtures/signing-session.json", "utf-8"));

describe.skipIf(!process.env.WINTERWALLET_INTEGRATION)("on-chain integration (surfpool)", () => {
  let surfnet: Surfnet;
  let connection: Connection;
  let payer: Keypair;

  beforeAll(async () => {
    surfnet = Surfnet.start();
    connection = new Connection(surfnet.rpcUrl, "confirmed");

    // Deploy winterwallet program from local .so
    await surfnet.deploy({
      programId: WINTERWALLET_PROGRAM_ID.toBase58(),
      soPath: "../../target/deploy/winterwallet.so",
    });

    // Fund payer
    payer = Keypair.generate();
    await surfnet.fundSol(payer.publicKey.toBase58(), 10_000_000_000);
  });

  afterAll(() => {
    surfnet?.stop?.();
  });

  it("initialize → withdraw → close lifecycle", async () => {
    const walletPda = new PublicKey(session.wallet_pda);
    const walletId = hexToBytes(session.wallet_id);

    // Step 1: Initialize
    const initSession = session.sessions[0];
    const initIx = createInitializeInstruction(
      payer.publicKey, walletPda,
      hexToBytes(initSession.signature),
      hexToBytes(initSession.new_root),
    );
    const initIxs = withComputeBudget([initIx]);
    // ... build legacy Transaction, sign with payer, send ...

    // Verify: account exists with correct root
    const account = await fetchWinterWalletAccount(connection, walletId);
    expect(hex(account.root)).toBe(initSession.new_root);

    // Step 2: Fund the PDA
    await surfnet.fundSol(walletPda.toBase58(), 5_000_000_000);

    // Step 3: Withdraw
    const withdrawSession = session.sessions[1];
    const receiver = Keypair.generate();
    const plan = createWithdrawPlan({
      walletPda, receiver: receiver.publicKey,
      lamports: BigInt(withdrawSession.lamports),
      newRoot: hexToBytes(withdrawSession.new_root),
    });

    // Verify digest matches fixture (cross-language parity check)
    const digest = await plan.digest(walletId, hexToBytes(withdrawSession.current_root));
    expect(hex(digest)).toBe(withdrawSession.digest);

    const advanceIx = plan.createInstruction(hexToBytes(withdrawSession.signature));
    // ... build, sign, send ...

    // Verify: root rotated
    const account2 = await fetchWinterWalletAccount(connection, walletId);
    expect(hex(account2.root)).toBe(withdrawSession.new_root);

    // Step 4: Close
    // ... similar pattern ...
    // Verify: account no longer exists
  });

  it("rejects invalid signature", async () => {
    // Use a valid fixture but corrupt the signature bytes
    // Expect transaction to fail with program error
  });
});
```

### Dependencies to Add

```json
// sdk/ts/package.json devDependencies
{
  "surfpool-sdk": "1.2.0"
}
```

**Pin to exact version (Architecture reviewer)**: surfpool is pre-1.0 — caret ranges on pre-1.0 packages allow breaking changes.

### Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `cli/src/helpers.rs` | Modify | Add stdin fallback to `read_mnemonic()` (Phase 0) |
| `client/tests/regen.rs` | Modify | Add `regen_signing_session` test |
| `fixtures/signing-session.json` | Create (generated) | Pre-signed Winternitz sessions (consider `.gitignore`) |
| `sdk/ts/test/integration.test.ts` | Create | Surfpool-based on-chain tests |
| `sdk/ts/package.json` | Modify | Add `surfpool-sdk` devDependency (exact version) |

### Test Coverage

| Test | What It Validates |
|------|------------------|
| Initialize | Instruction encoding → on-chain account creation, correct root stored |
| Advance(Withdraw) | Payload encoding, signature verification, SOL transfer, root rotation |
| Advance(Close) | Account destruction, lamport sweep, data zeroing |
| State verification | `fetchWinterWalletAccount()` returns correct id, root, bump between steps |
| Cross-language digest | TS-computed digest matches Rust-generated fixture digest |
| Error: invalid signature | Transaction rejected with program error (wrong signature bytes) |

### Acceptance Criteria

- [x] `fixtures/signing-session.json` is generated by `cargo test -p winterwallet-client --test regen -- --ignored regen_signing_session --nocapture`
- [ ] `WINTERWALLET_INTEGRATION=1 bun run test` in `sdk/ts/` passes the full lifecycle test
- [x] Integration tests run against embedded surfpool (no external process)
- [x] Tests verify on-chain state after each step (root, balance, account existence)
- [x] Integration tests are skipped by default (`bun run test` without env var)
- [x] At least one error path is tested (invalid signature rejection)
- [x] Cross-language digest parity is verified (TS digest matches Rust fixture)

---

## Execution Order

```
Phase 0: CLI stdin fallback ──────────────────────────────
  └─ Modify cli/src/helpers.rs (prerequisite for Phase 1)

Phase 1: CLI Smoke Test ──────────────────────────────────
  ├─ Create scripts/smoke.sh
  ├─ Build program .so for surfpool
  ├─ Test against surfpool locally
  └─ Test against devnet

Phase 2: ALT Support ─────────────────────────────────────
  ├─ Add private partitionV0Accounts to transaction.ts
  ├─ Add estimateV0TransactionSize + assertV0TransactionSize
  ├─ Add unit tests
  └─ Verify size estimation matches real VersionedTransaction

Phase 3: Surfpool Integration Tests ──────────────────────
  ├─ Extend regen.rs with signing session generation
  ├─ Generate fixtures/signing-session.json
  ├─ Add surfpool-sdk dependency (pinned)
  ├─ Write integration.test.ts
  └─ Verify lifecycle passes on surfpool
```

Phase 0 must complete before Phase 1. Phase 1, 2, and 3 are independent of each other after that. Phase 3 can optionally test ALT paths once Phase 2 is done.

## Future Work (Tracked, Not Blocking)

- **Rust client v0 parity**: Add `estimate_v0_transaction_size` to `client/src/transaction.rs` when the CLI needs to support more complex multi-CPI advances. Add a `// Note: v0 estimation is TS-only. Rust client deferred.` comment in the TS code.
- **V0 golden vector fixtures**: Add alongside legacy once Rust client gains v0 support.
- **CI fixture staleness detection**: Add a CI step that runs `cargo test --ignored` and checks `git diff --exit-code fixtures/` to detect stale fixtures.
- **Token-2022 testing**: Smoke test only for now; add TS integration later if needed.
- **`recover` command testing**: Add to smoke test as a separate scenario.

## References

- CLI entry + flags: `cli/src/main.rs:15-37`
- CLI mnemonic read: `cli/src/helpers.rs:37-43`
- TS transaction size estimation: `sdk/ts/src/transaction.ts:77-98`
- TS AdvancePlan: `sdk/ts/src/plan.ts:33-95`
- TS advance encoding: `sdk/ts/src/instructions/advance.ts:115-163`
- Rust fixture regenerator: `client/tests/regen.rs`
- On-chain advance handler (preimage binding): `program/src/instructions/advance.rs:56-143`
- Shared constants: `common/src/lib.rs`
- `@solana/web3.js` v1.x types: `node_modules/@solana/web3.js/lib/index.d.ts` (MessageV0, VersionedTransaction, AddressLookupTableAccount)
- msig-cli smoke test (reference): `/Users/leo/Documents/github-work/msig-cli/scripts/devnet-smoke.sh`
