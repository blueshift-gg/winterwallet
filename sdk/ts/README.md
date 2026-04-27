# @blueshift-gg/winterwallet

TypeScript SDK for [WinterWallet](https://github.com/blueshift-gg/winterwallet) — a Solana program that turns a single Winternitz one-time signature into a full transaction's worth of CPI authority. This package builds the on-wire instructions, derives PDAs, encodes the Advance payload, and computes the preimage you sign over with your Winternitz keypair.

It is a pure builder library: no RPC client, no key management, no signing. Bring your own `Connection`, your own Winternitz signing stack, and your own transaction sender.

## Install

```sh
bun add @blueshift-gg/winterwallet @solana/web3.js
# or: npm install / pnpm add / yarn add
```

`@solana/web3.js` v1 and `@solana/codecs` are required at runtime.

## Quickstart

### 1. Derive the wallet PDA

```ts
import { findWinterWalletPda } from "@blueshift-gg/winterwallet";

const walletId = new Uint8Array(32); // 32-byte merklized Winternitz pubkey root
const [walletPda, bump] = findWinterWalletPda(walletId);
```

### 2. Build an Initialize instruction

```ts
import {
  SIGNATURE_LEN,
  createInitializeInstruction,
  initializeDigest,
} from "@blueshift-gg/winterwallet";

const digest = await initializeDigest();             // SHA-256 of the domain tag
const signatureBytes = new Uint8Array(SIGNATURE_LEN); // sign `digest` off-chain
const nextRoot = new Uint8Array(32);                  // commitment for the next position

const ix = createInitializeInstruction(payer, walletPda, signatureBytes, nextRoot);
```

### 3. Build an Advance plan

`AdvancePlan` owns the encoded CPI payload **and** the matching account list, so you can't accidentally sign one ordering and submit another.

```ts
import { createWithdrawPlan } from "@blueshift-gg/winterwallet";

const plan = createWithdrawPlan({
  walletPda,
  receiver,
  lamports: 500_000n,
  newRoot, // commitment for the position AFTER this signature
});

// Compute the preimage digest that the Winternitz signature must commit to.
const digest = await plan.digest(walletId, currentRoot);

// ...sign `digest` with your Winternitz keypair, then:
const advanceIx = plan.createInstruction(signatureBytes);
```

### 4. Wrap arbitrary inner CPIs

`createAdvancePlan` accepts any list of `TransactionInstruction`s. The wallet PDA is promoted to signer on-chain via `invoke_signed`.

```ts
import { createAdvancePlan } from "@blueshift-gg/winterwallet";

const plan = createAdvancePlan({
  walletPda,
  newRoot,
  innerInstructions: [
    splTokenTransferIx, // wallet PDA appears as the source authority
    anotherInstruction,
  ],
});
```

### 5. Close the wallet

```ts
import { createClosePlan } from "@blueshift-gg/winterwallet";

const plan = createClosePlan({ walletPda, receiver, newRoot });
// Sweeps all lamports to `receiver` and tears the PDA down.
```

## API

| Export | Purpose |
|---|---|
| `findWinterWalletPda(walletId)` | Derive the wallet PDA + bump. |
| `fetchWinterWalletAccount(connection, walletId)` | Fetch + deserialize on-chain wallet state. |
| `deserializeWinterWalletAccount(bytes)` | Decode a 65-byte wallet account. |
| `createInitializeInstruction(...)` | Build the Initialize instruction. |
| `createWithdrawInstruction(walletPda, receiver, lamports)` | Inner CPI: lamport withdraw. |
| `createCloseInstruction(walletPda, receiver)` | Inner CPI: sweep all lamports + tear down. |
| `createAdvanceInstruction(...)` | Build a top-level Advance instruction directly. |
| `encodeAdvancePayload(innerInstructions)` | Encode CPI payload + account list atomically. |
| `initializeDigest()` | SHA-256 preimage digest for Initialize signatures. |
| `advanceDigest(...)` | SHA-256 preimage digest for Advance signatures. |
| `AdvancePlan` / `createAdvancePlan` | Holds payload + accounts + helpers (digest, instruction, tx-size estimate). |
| `createWithdrawPlan` / `createClosePlan` | Convenience plan factories. |
| `WinterWalletClient` | Optional thin wrapper over `Connection` for fetch + plan building. |
| `withComputeBudget`, `estimateLegacyTransactionSize`, `assertLegacyTransactionSize` | Transaction-shape helpers. |
| `WinterWalletError` | All SDK errors. |

All sizes, discriminators, domain tags, and limits are exported as named constants from `constants` (e.g. `SIGNATURE_LEN`, `WALLET_ACCOUNT_LEN`, `WINTERWALLET_PROGRAM_ID`).

## Notes

- **Crypto.** Digests use the platform's Web Crypto (`globalThis.crypto.subtle`); no Node-specific APIs are required. Available in Node 20+ and all modern browsers.
- **Inner-CPI signer flags.** `encodeAdvancePayload` scrubs `isSigner` on every passthrough account. The on-chain handler promotes the wallet PDA to signer via `invoke_signed`.
- **Position discipline.** Winternitz signatures are one-time. Reusing a position weakens security; always advance off-chain key state in lockstep with on-chain root advancement.

## License

MIT
