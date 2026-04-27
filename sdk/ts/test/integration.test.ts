/**
 * On-chain integration tests using surfpool CLI.
 *
 * Runs the full wallet lifecycle against a surfpool instance:
 *   initialize → fund PDA → withdraw → token transfer → close
 *
 * The winterwallet program is already deployed on mainnet — surfpool
 * clones it automatically. No local build-sbf needed.
 *
 * Run with:
 *   WINTERWALLET_INTEGRATION=1 bun run test
 *
 * Prerequisites:
 *   - surfpool CLI v1.2.0+ installed (curl -fsSL https://run.surfpool.run | sh)
 *   - cargo test -p winterwallet-client --test regen -- --ignored regen_signing_session --nocapture
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { readFileSync } from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import {
  AddressLookupTableAccount,
  AddressLookupTableProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
  sendAndConfirmTransaction,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  WINTERWALLET_PROGRAM_ID,
  COMPUTE_BUDGET_PROGRAM_ID,
  createInitializeInstruction,
  AdvancePlan,
  createWithdrawPlan,
  createClosePlan,
  withComputeBudget,
  estimateLegacyTransactionSize,
  estimateV0TransactionSize,
  fetchWinterWalletAccount,
} from "../src/index.js";

// ── Fixture loading ─────────────────────────────────────────────────

interface SessionEntry {
  current_root: string;
  digest: string;
  new_root: string;
  position: number;
  signature: string;
  lamports?: string;
  passthrough_accounts?: { pubkey: string; is_signer: boolean; is_writable: boolean }[];
  payload?: string;
}

interface SigningSession {
  wallet_pda: string;
  wallet_id: string;
  wallet_bump: number;
  receiver: string;
  payer: string;
  lamports: string;
  mint: string;
  source_ata: string;
  destination_ata: string;
  token_program: string;
  token_amount: string;
  sessions: SessionEntry[];
}

const SESSION_PATH = new URL(
  "../../../fixtures/signing-session.json",
  import.meta.url,
);

function loadSession(): SigningSession {
  return JSON.parse(readFileSync(SESSION_PATH, "utf-8"));
}

function hexToBytes(value: string): Uint8Array {
  return Uint8Array.from(Buffer.from(value, "hex"));
}

function hex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("hex");
}

// ── Surfpool helpers ────────────────────────────────────────────────

const RPC_URL = "http://127.0.0.1:8899";

async function waitForRpc(url: string, timeoutMs: number): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "getHealth" }),
      });
      if (res.ok) {
        const json = (await res.json()) as { result?: string };
        if (json.result === "ok") return;
      }
    } catch {
      // not ready yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`RPC at ${url} not ready after ${timeoutMs}ms`);
}

/** Fund a system account using surfpool's surfnet_setAccount cheatcode. */
async function fundAccount(
  url: string,
  pubkey: string,
  lamports: number,
): Promise<void> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "surfnet_setAccount",
      params: [
        pubkey,
        {
          lamports,
          data: "",
          owner: "11111111111111111111111111111111",
          executable: false,
          rentEpoch: 0,
        },
      ],
    }),
  });
  const json = (await res.json()) as { error?: unknown };
  if (json.error) {
    throw new Error(`surfnet_setAccount failed: ${JSON.stringify(json.error)}`);
  }
}

/** Set up a token account with a balance via surfnet_setTokenAccount. */
async function setTokenBalance(
  url: string,
  owner: string,
  mint: string,
  amount: number,
  tokenProgram?: string,
): Promise<void> {
  const params: (string | { amount: number })[] = [owner, mint, { amount }];
  if (tokenProgram) params.push(tokenProgram);
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "surfnet_setTokenAccount",
      params,
    }),
  });
  const json = (await res.json()) as { error?: unknown };
  if (json.error) {
    throw new Error(
      `surfnet_setTokenAccount failed: ${JSON.stringify(json.error)}`,
    );
  }
}

/** Send a legacy transaction with detailed error logging on failure. */
async function sendTx(
  connection: Connection,
  tx: Transaction,
  signers: Keypair[],
  label: string,
): Promise<void> {
  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  tx.feePayer = signers[0].publicKey;
  tx.sign(...signers);

  const sig = await connection.sendRawTransaction(tx.serialize(), {
    skipPreflight: true,
  });
  const result = await connection.confirmTransaction(sig, "confirmed");
  if (result.value.err) {
    const logs = await connection.getTransaction(sig, {
      commitment: "confirmed",
    });
    console.error(`${label} error:`, JSON.stringify(result.value.err));
    console.error(`${label} logs:`, logs?.meta?.logMessages);
    throw new Error(`${label} failed: ${JSON.stringify(result.value.err)}`);
  }
}

/** Send a v0 versioned transaction with ALT lookups. */
async function sendV0Tx(
  connection: Connection,
  instructions: TransactionInstruction[],
  signers: Keypair[],
  lookupTables: AddressLookupTableAccount[],
  label: string,
): Promise<void> {
  const { blockhash } = await connection.getLatestBlockhash();
  const messageV0 = new TransactionMessage({
    payerKey: signers[0].publicKey,
    recentBlockhash: blockhash,
    instructions,
  }).compileToV0Message(lookupTables);
  const tx = new VersionedTransaction(messageV0);
  tx.sign(signers);

  const sig = await connection.sendRawTransaction(tx.serialize(), {
    skipPreflight: true,
  });
  const result = await connection.confirmTransaction(sig, "confirmed");
  if (result.value.err) {
    const logs = await connection.getTransaction(sig, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0,
    });
    console.error(`${label} error:`, JSON.stringify(result.value.err));
    console.error(`${label} logs:`, logs?.meta?.logMessages);
    throw new Error(`${label} failed: ${JSON.stringify(result.value.err)}`);
  }
}

/** Create an on-chain ALT, extend it with addresses, and return it. */
async function createAlt(
  connection: Connection,
  payer: Keypair,
  addresses: PublicKey[],
): Promise<AddressLookupTableAccount> {
  const slot = await connection.getSlot();
  const [createIx, tableAddress] = AddressLookupTableProgram.createLookupTable({
    authority: payer.publicKey,
    payer: payer.publicKey,
    recentSlot: slot,
  });
  const extendIx = AddressLookupTableProgram.extendLookupTable({
    payer: payer.publicKey,
    authority: payer.publicKey,
    lookupTable: tableAddress,
    addresses,
  });
  await sendTx(
    connection,
    new Transaction().add(createIx, extendIx),
    [payer],
    "Create ALT",
  );

  // ALTs need one slot to activate. Poll until ready.
  for (let i = 0; i < 20; i++) {
    const response = await connection.getAddressLookupTable(tableAddress);
    if (response.value && response.value.state.addresses.length > 0) {
      return response.value;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("ALT not active after 10s");
}

function loadSessionFile(name: string): SigningSession {
  const path = new URL(`../../../fixtures/${name}`, import.meta.url);
  return JSON.parse(readFileSync(path, "utf-8"));
}

// ── Tests ───────────────────────────────────────────────────────────

describe.skipIf(!process.env.WINTERWALLET_INTEGRATION)(
  "on-chain integration (surfpool)",
  () => {
    let surfpoolProcess: ChildProcess | null = null;
    let connection: Connection;
    let payer: Keypair;
    let session: SigningSession;

    beforeAll(async () => {
      // Kill any stale surfpool on our port before starting.
      const { execSync } = await import("node:child_process");
      try {
        execSync(`lsof -ti :8899 | xargs kill -9`, { stdio: "ignore" });
        await new Promise((r) => setTimeout(r, 500));
      } catch {
        // nothing running — fine
      }

      // Start surfpool CLI. The winterwallet program is on mainnet so
      // surfpool clones it automatically. --features-all activates all
      // SVM feature gates (including disable_rent_fees_collection to
      // work around pinocchio's rent calculation mismatch).
      surfpoolProcess = spawn(
        "surfpool",
        [
          "start",
          "--ci",
          "--no-tui",
          "--no-studio",
          "--no-deploy",
          "--features-all",
        ],
        { stdio: "pipe", detached: false },
      );

      await waitForRpc(RPC_URL, 30_000);
      connection = new Connection(RPC_URL, "confirmed");

      // Generate a fresh payer keypair and fund via surfnet_setAccount.
      // This is the only cheatcode — on mainnet the payer buys SOL.
      payer = Keypair.generate();
      await fundAccount(
        RPC_URL,
        payer.publicKey.toBase58(),
        100 * LAMPORTS_PER_SOL,
      );

      // Load the pre-generated signing session fixture.
      session = loadSession();
    }, 60_000);

    afterAll(() => {
      if (surfpoolProcess) {
        surfpoolProcess.kill("SIGTERM");
        surfpoolProcess = null;
      }
    });

    it(
      "initialize → fund → withdraw → token transfer → close lifecycle",
      async () => {
        const walletPda = new PublicKey(session.wallet_pda);
        const walletId = hexToBytes(session.wallet_id);
        const receiver = new PublicKey(session.receiver);

        // Winternitz sig verification is compute-intensive.
        const cuLimit = 1_400_000;

        // ── Step 1: Initialize ──────────────────────────────────
        const initSession = session.sessions[0];
        const initIx = createInitializeInstruction(
          payer.publicKey,
          walletPda,
          hexToBytes(initSession.signature),
          hexToBytes(initSession.new_root),
        );

        const initIxs = withComputeBudget([initIx], cuLimit);
        console.log(`Initialize:     ${estimateLegacyTransactionSize(payer.publicKey, initIxs)} bytes`);

        await sendTx(
          connection,
          new Transaction().add(...initIxs),
          [payer],
          "Initialize",
        );

        // Verify: account exists with correct root.
        const account1 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account1.root)).toBe(initSession.new_root);
        expect(account1.bump).toBe(session.wallet_bump);

        // ── Step 2: Fund the PDA (real transfer, like mainnet) ──
        const fundIx = SystemProgram.transfer({
          fromPubkey: payer.publicKey,
          toPubkey: walletPda,
          lamports: 10 * LAMPORTS_PER_SOL,
        });
        console.log(`Fund PDA:       ${estimateLegacyTransactionSize(payer.publicKey, [fundIx])} bytes`);

        await sendTx(
          connection,
          new Transaction().add(fundIx),
          [payer],
          "Fund PDA",
        );

        // ── Step 3: Withdraw ────────────────────────────────────
        const withdrawSession = session.sessions[1];
        const withdrawPlan = createWithdrawPlan({
          walletPda,
          receiver,
          lamports: BigInt(session.lamports),
          newRoot: hexToBytes(withdrawSession.new_root),
        });

        // Cross-language digest parity.
        const withdrawDigest = await withdrawPlan.digest(
          walletId,
          hexToBytes(withdrawSession.current_root),
        );
        expect(hex(withdrawDigest)).toBe(withdrawSession.digest);

        const withdrawIxs = withdrawPlan.withComputeBudget(
          hexToBytes(withdrawSession.signature),
          cuLimit,
        );
        console.log(`Withdraw:       ${estimateLegacyTransactionSize(payer.publicKey, withdrawIxs)} bytes`);

        await sendTx(
          connection,
          new Transaction().add(...withdrawIxs),
          [payer],
          "Withdraw",
        );

        // Verify: root rotated.
        const account2 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account2.root)).toBe(withdrawSession.new_root);

        // ── Step 4: Token Transfer ──────────────────────────────
        const tokenSession = session.sessions[2];
        const mint = new PublicKey(session.mint);
        const sourceAta = new PublicKey(session.source_ata);
        const destinationAta = new PublicKey(session.destination_ata);
        const tokenProgram = new PublicKey(session.token_program);

        // Set up token accounts via surfpool cheatcode.
        // On mainnet these would already exist.
        await setTokenBalance(
          RPC_URL,
          walletPda.toBase58(),
          mint.toBase58(),
          Number(session.token_amount) * 10, // fund more than we transfer
        );
        await setTokenBalance(
          RPC_URL,
          receiver.toBase58(),
          mint.toBase58(),
          0, // destination starts empty
        );

        // Build the SPL Token Transfer inner instruction.
        const tokenTransferIx = new TransactionInstruction({
          programId: tokenProgram,
          keys: [
            { pubkey: sourceAta, isSigner: false, isWritable: true },
            { pubkey: destinationAta, isSigner: false, isWritable: true },
            { pubkey: walletPda, isSigner: false, isWritable: false },
          ],
          data: Buffer.concat([
            Buffer.from([3]), // Transfer discriminator
            Buffer.from(
              new BigUint64Array([BigInt(session.token_amount)]).buffer,
            ),
          ]),
        });

        const tokenPlan = new AdvancePlan({
          walletPda,
          newRoot: hexToBytes(tokenSession.new_root),
          innerInstructions: [tokenTransferIx],
        });

        // Cross-language digest parity.
        const tokenDigest = await tokenPlan.digest(
          walletId,
          hexToBytes(tokenSession.current_root),
        );
        expect(hex(tokenDigest)).toBe(tokenSession.digest);

        const tokenIxs = tokenPlan.withComputeBudget(
          hexToBytes(tokenSession.signature),
          cuLimit,
        );
        console.log(`Token Transfer: ${estimateLegacyTransactionSize(payer.publicKey, tokenIxs)} bytes`);

        await sendTx(
          connection,
          new Transaction().add(...tokenIxs),
          [payer],
          "Token Transfer",
        );

        // Verify: root rotated.
        const account3 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account3.root)).toBe(tokenSession.new_root);

        // ── Step 5: Close ───────────────────────────────────────
        const closeSession = session.sessions[3];
        const closePlan = createClosePlan({
          walletPda,
          receiver,
          newRoot: hexToBytes(closeSession.new_root),
        });

        // Cross-language digest parity.
        const closeDigest = await closePlan.digest(
          walletId,
          hexToBytes(closeSession.current_root),
        );
        expect(hex(closeDigest)).toBe(closeSession.digest);

        const closeIxs = closePlan.withComputeBudget(
          hexToBytes(closeSession.signature),
          cuLimit,
        );
        console.log(`Close:          ${estimateLegacyTransactionSize(payer.publicKey, closeIxs)} bytes`);

        await sendTx(
          connection,
          new Transaction().add(...closeIxs),
          [payer],
          "Close",
        );

        // Verify: account destroyed.
        const info = await connection.getAccountInfo(walletPda);
        expect(info).toBeNull();
      },
      120_000,
    );

    it(
      "rejects invalid signature",
      async () => {
        const initSession = session.sessions[0];
        const corruptSig = hexToBytes(initSession.signature);
        corruptSig[0] ^= 0xff;

        // Fresh wallet PDA to avoid collision with lifecycle test.
        const freshId = new Uint8Array(32);
        crypto.getRandomValues(freshId);
        const [freshPda] = PublicKey.findProgramAddressSync(
          [new TextEncoder().encode("winterwallet"), freshId],
          WINTERWALLET_PROGRAM_ID,
        );

        const ix = createInitializeInstruction(
          payer.publicKey,
          freshPda,
          corruptSig,
          hexToBytes(initSession.new_root),
        );
        const tx = new Transaction().add(
          ...withComputeBudget([ix], 1_400_000),
        );

        await expect(
          sendAndConfirmTransaction(connection, tx, [payer]),
        ).rejects.toThrow();
      },
      30_000,
    );

    it(
      "v0 + ALT: initialize → fund → withdraw → token transfer → close",
      async () => {
        const s = loadSessionFile("signing-session-alt1.json");
        const walletPda = new PublicKey(s.wallet_pda);
        const walletId = hexToBytes(s.wallet_id);
        const receiver = new PublicKey(s.receiver);
        const cuLimit = 1_400_000;

        // ── Initialize (legacy — wallet doesn't exist yet for ALT) ──
        const initSession = s.sessions[0];
        const initIx = createInitializeInstruction(
          payer.publicKey,
          walletPda,
          hexToBytes(initSession.signature),
          hexToBytes(initSession.new_root),
        );
        const initIxs = withComputeBudget([initIx], cuLimit);
        console.log(`[ALT1] Initialize (legacy): ${estimateLegacyTransactionSize(payer.publicKey, initIxs)} bytes`);
        await sendTx(connection, new Transaction().add(...initIxs), [payer], "ALT1 Initialize");

        const account1 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account1.root)).toBe(initSession.new_root);

        // ── Fund PDA ──
        const fundIx = SystemProgram.transfer({
          fromPubkey: payer.publicKey, toPubkey: walletPda, lamports: 10 * LAMPORTS_PER_SOL,
        });
        await sendTx(connection, new Transaction().add(fundIx), [payer], "ALT1 Fund PDA");

        // ── Create ALT with all non-signer accounts ──
        const mint = new PublicKey(s.mint);
        const sourceAta = new PublicKey(s.source_ata);
        const destinationAta = new PublicKey(s.destination_ata);
        const tokenProgram = new PublicKey(s.token_program);

        await setTokenBalance(RPC_URL, walletPda.toBase58(), mint.toBase58(), Number(s.token_amount) * 10);
        await setTokenBalance(RPC_URL, receiver.toBase58(), mint.toBase58(), 0);

        const alt = await createAlt(connection, payer, [
          walletPda, receiver, WINTERWALLET_PROGRAM_ID, SystemProgram.programId,
          COMPUTE_BUDGET_PROGRAM_ID, sourceAta, destinationAta, tokenProgram, mint,
        ]);

        // ── Withdraw (v0) ──
        const withdrawSession = s.sessions[1];
        const withdrawPlan = createWithdrawPlan({
          walletPda, receiver, lamports: BigInt(s.lamports),
          newRoot: hexToBytes(withdrawSession.new_root),
        });
        const withdrawIxs = withdrawPlan.withComputeBudget(hexToBytes(withdrawSession.signature), cuLimit);
        const withdrawLegacy = estimateLegacyTransactionSize(payer.publicKey, withdrawIxs);
        const withdrawV0 = estimateV0TransactionSize(payer.publicKey, withdrawIxs, [alt]);
        console.log(`[ALT1] Withdraw:        legacy=${withdrawLegacy} v0=${withdrawV0} saved=${withdrawLegacy - withdrawV0} bytes`);
        await sendV0Tx(connection, withdrawIxs, [payer], [alt], "ALT1 Withdraw");

        const account2 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account2.root)).toBe(withdrawSession.new_root);

        // ── Token Transfer (v0) ──
        const tokenSession = s.sessions[2];
        const tokenTransferIx = new TransactionInstruction({
          programId: tokenProgram,
          keys: [
            { pubkey: sourceAta, isSigner: false, isWritable: true },
            { pubkey: destinationAta, isSigner: false, isWritable: true },
            { pubkey: walletPda, isSigner: false, isWritable: false },
          ],
          data: Buffer.concat([
            Buffer.from([3]),
            Buffer.from(new BigUint64Array([BigInt(s.token_amount)]).buffer),
          ]),
        });
        const tokenPlan = new AdvancePlan({
          walletPda, newRoot: hexToBytes(tokenSession.new_root),
          innerInstructions: [tokenTransferIx],
        });
        const tokenIxs = tokenPlan.withComputeBudget(hexToBytes(tokenSession.signature), cuLimit);
        const tokenLegacy = estimateLegacyTransactionSize(payer.publicKey, tokenIxs);
        const tokenV0 = estimateV0TransactionSize(payer.publicKey, tokenIxs, [alt]);
        console.log(`[ALT1] Token Transfer:  legacy=${tokenLegacy} v0=${tokenV0} saved=${tokenLegacy - tokenV0} bytes`);
        await sendV0Tx(connection, tokenIxs, [payer], [alt], "ALT1 Token Transfer");

        const account3 = await fetchWinterWalletAccount(connection, walletId);
        expect(hex(account3.root)).toBe(tokenSession.new_root);

        // ── Close (v0) ──
        const closeSession = s.sessions[3];
        const closePlan = createClosePlan({
          walletPda, receiver, newRoot: hexToBytes(closeSession.new_root),
        });
        const closeIxs = closePlan.withComputeBudget(hexToBytes(closeSession.signature), cuLimit);
        const closeLegacy = estimateLegacyTransactionSize(payer.publicKey, closeIxs);
        const closeV0 = estimateV0TransactionSize(payer.publicKey, closeIxs, [alt]);
        console.log(`[ALT1] Close:           legacy=${closeLegacy} v0=${closeV0} saved=${closeLegacy - closeV0} bytes`);
        await sendV0Tx(connection, closeIxs, [payer], [alt], "ALT1 Close");

        const info = await connection.getAccountInfo(walletPda);
        expect(info).toBeNull();
      },
      120_000,
    );

  },
);
