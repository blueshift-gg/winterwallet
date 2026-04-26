import {
  Connection,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  type AccountMeta,
} from "@solana/web3.js";
import { sha256 } from "@noble/hashes/sha256";

// ── Constants ────────────────────────────────────────────────────────

export const WINTERWALLET_PROGRAM_ID = new PublicKey(
  "22222222222222222222222222222222222222222222"
);

export const INITIALIZE_DISCRIMINATOR = 0;
export const ADVANCE_DISCRIMINATOR = 1;
export const WITHDRAW_DISCRIMINATOR = 2;

export const WINTERNITZ_SCALARS = 22;
export const TOTAL_SCALARS = WINTERNITZ_SCALARS + 2;
export const SIGNATURE_LEN = TOTAL_SCALARS * 32; // 768
export const WALLET_ACCOUNT_LEN = 65;

export const WINTERWALLET_SEED = new TextEncoder().encode("winterwallet");
export const WINTERWALLET_INITIALIZE_TAG = new TextEncoder().encode(
  "WINTERWALLET_INITIALIZE"
);
export const WINTERWALLET_ADVANCE_TAG = new TextEncoder().encode(
  "WINTERWALLET_ADVANCE"
);

export const MAX_PASSTHROUGH_ACCOUNTS = 128;
export const MAX_CPI_INSTRUCTION_ACCOUNTS = 16;
export const LEGACY_TRANSACTION_SIZE_LIMIT = 1232;
export const DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT = 400_000;

export const COMPUTE_BUDGET_PROGRAM_ID = new PublicKey(
  "ComputeBudget111111111111111111111111111111"
);

// ── Branded types ────────────────────────────────────────────────────

/** 32-byte wallet identifier (merklized Winternitz pubkey root). */
export type WalletId = Uint8Array & { readonly __brand: "WalletId" };

/** 32-byte Winternitz merkle root. */
export type WinternitzRoot = Uint8Array & {
  readonly __brand: "WinternitzRoot";
};

// ── WinterWalletAccount ──────────────────────────────────────────────

export interface WinterWalletAccount {
  readonly id: WalletId;
  readonly root: WinternitzRoot;
  readonly bump: number;
}

export function deserializeWinterWalletAccount(
  data: Uint8Array
): WinterWalletAccount {
  if (data.length !== WALLET_ACCOUNT_LEN) {
    throw new WinterWalletError(
      `Account data must be exactly ${WALLET_ACCOUNT_LEN} bytes, got ${data.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
  return {
    id: data.slice(0, 32) as WalletId,
    root: data.slice(32, 64) as WinternitzRoot,
    bump: data[64],
  };
}

// ── PDA ──────────────────────────────────────────────────────────────

export function findWinterWalletPda(
  walletId: Uint8Array
): [PublicKey, number] {
  assert32(walletId, "walletId");
  return PublicKey.findProgramAddressSync(
    [WINTERWALLET_SEED, walletId],
    WINTERWALLET_PROGRAM_ID
  );
}

// ── Query ────────────────────────────────────────────────────────────

export async function fetchWinterWalletAccount(
  connection: Connection,
  walletId: Uint8Array
): Promise<WinterWalletAccount> {
  const [pda] = findWinterWalletPda(walletId);
  const info = await connection.getAccountInfo(pda);
  if (!info) {
    throw new WinterWalletError(
      `WinterWallet account not found for PDA ${pda.toBase58()}`,
      "ACCOUNT_NOT_FOUND"
    );
  }
  return deserializeWinterWalletAccount(
    info.data
  );
}

// ── Instructions ─────────────────────────────────────────────────────

/**
 * Build an Initialize instruction.
 *
 * Accounts: [payer (signer, writable), walletPda (writable), systemProgram].
 */
export function createInitializeInstruction(
  payer: PublicKey,
  walletPda: PublicKey,
  signatureBytes: Uint8Array,
  nextRoot: Uint8Array
): TransactionInstruction {
  assertLen(signatureBytes, SIGNATURE_LEN, "signatureBytes");
  assert32(nextRoot, "nextRoot");

  const data = new Uint8Array(1 + SIGNATURE_LEN + 32);
  data[0] = INITIALIZE_DISCRIMINATOR;
  data.set(signatureBytes, 1);
  data.set(nextRoot, 1 + SIGNATURE_LEN);

  return new TransactionInstruction({
    programId: WINTERWALLET_PROGRAM_ID,
    keys: [
      { pubkey: payer, isSigner: true, isWritable: true },
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(data),
  });
}

/**
 * Build an Advance instruction wrapping inner CPI instructions.
 *
 * Accepts inner instructions directly and handles payload encoding +
 * account ordering atomically. Signer flags on inner accounts are
 * scrubbed — the on-chain handler promotes the wallet PDA to signer
 * via `invoke_signed`.
 */
export function createAdvanceInstruction(
  walletPda: PublicKey,
  signatureBytes: Uint8Array,
  newRoot: Uint8Array,
  innerInstructions: TransactionInstruction[]
): TransactionInstruction {
  assertLen(signatureBytes, SIGNATURE_LEN, "signatureBytes");
  assert32(newRoot, "newRoot");

  if (innerInstructions.length > 255) {
    throw new WinterWalletError(
      `Too many inner instructions: ${innerInstructions.length} (max 255)`,
      "PAYLOAD_TOO_LARGE"
    );
  }

  const { payload, accounts } = encodeAdvancePayload(innerInstructions);

  const data = new Uint8Array(1 + SIGNATURE_LEN + 32 + payload.length);
  let off = 0;
  data[off++] = ADVANCE_DISCRIMINATOR;
  data.set(signatureBytes, off);
  off += SIGNATURE_LEN;
  data.set(newRoot, off);
  off += 32;
  data.set(payload, off);

  const keys: AccountMeta[] = [
    { pubkey: walletPda, isSigner: false, isWritable: true },
    ...accounts,
  ];

  return new TransactionInstruction({
    programId: WINTERWALLET_PROGRAM_ID,
    keys,
    data: Buffer.from(data),
  });
}

/**
 * Build a Withdraw instruction for use as an inner CPI inside Advance.
 *
 * Pass the result to `createAdvanceInstruction` as one of the inner
 * instructions. Do NOT submit this as a top-level instruction. The wallet PDA
 * is not marked as a transaction signer; the program promotes it on-chain via
 * `invoke_signed`.
 */
export function createWithdrawInstruction(
  walletPda: PublicKey,
  receiver: PublicKey,
  lamports: bigint
): TransactionInstruction {
  assertU64(lamports, "lamports");
  const data = new Uint8Array(1 + 8);
  data[0] = WITHDRAW_DISCRIMINATOR;
  writeU64LE(data, lamports, 1);

  return new TransactionInstruction({
    programId: WINTERWALLET_PROGRAM_ID,
    keys: [
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: receiver, isSigner: false, isWritable: true },
    ],
    data: Buffer.from(data),
  });
}

// ── Payload Encoder ──────────────────────────────────────────────────

interface AdvancePayloadResult {
  payload: Uint8Array;
  accounts: AccountMeta[];
}

/**
 * Encode inner CPI instructions into Advance payload bytes + ordered
 * account list. Both are produced atomically to prevent ordering
 * mismatches. Signer flags are scrubbed — the on-chain handler promotes
 * the wallet PDA via `invoke_signed`.
 */
export function encodeAdvancePayload(
  innerInstructions: TransactionInstruction[]
): AdvancePayloadResult {
  // Validate total passthrough accounts.
  let totalAccounts = 0;
  for (const ix of innerInstructions) {
    totalAccounts += 1 + ix.keys.length; // +1 for program_id account
  }
  if (totalAccounts > MAX_PASSTHROUGH_ACCOUNTS) {
    throw new WinterWalletError(
      `Total passthrough accounts (${totalAccounts}) exceeds MAX_PASSTHROUGH_ACCOUNTS (${MAX_PASSTHROUGH_ACCOUNTS})`,
      "PAYLOAD_TOO_LARGE"
    );
  }

  let payloadLen = 1;
  for (const ix of innerInstructions) {
    payloadLen += 1 + 2 + ix.data.length;
  }

  const payload = new Uint8Array(payloadLen);
  const accounts: AccountMeta[] = [];
  let off = 0;

  payload[off++] = innerInstructions.length;

  for (const ix of innerInstructions) {
    if (ix.keys.length > MAX_CPI_INSTRUCTION_ACCOUNTS) {
      throw new WinterWalletError(
        `Inner instruction has ${ix.keys.length} accounts (max ${MAX_CPI_INSTRUCTION_ACCOUNTS})`,
        "PAYLOAD_TOO_LARGE"
      );
    }
    if (ix.data.length > 65535) {
      throw new WinterWalletError(
        `Inner instruction data too long: ${ix.data.length} (max 65535)`,
        "PAYLOAD_TOO_LARGE"
      );
    }

    payload[off++] = ix.keys.length;
    writeU16LE(payload, ix.data.length, off);
    off += 2;
    payload.set(ix.data, off);
    off += ix.data.length;

    // Program ID account (readonly, not signer).
    accounts.push({
      pubkey: ix.programId,
      isSigner: false,
      isWritable: false,
    });
    // Scrub signer flags — PDA signing happens via invoke_signed on-chain.
    for (const meta of ix.keys) {
      accounts.push({
        pubkey: meta.pubkey,
        isSigner: false,
        isWritable: meta.isWritable,
      });
    }
  }

  return { payload, accounts };
}

// ── Preimage / Digest ────────────────────────────────────────────────

/**
 * Compute the Initialize preimage digest (SHA-256).
 *
 * The program signs over just the domain tag. The wallet ID is recovered
 * from the signature, not committed in the preimage.
 */
export function initializeDigest(): Uint8Array {
  return sha256(WINTERWALLET_INITIALIZE_TAG);
}

/**
 * Compute the Advance preimage digest (SHA-256).
 *
 * Must match `program/src/instructions/advance.rs:verify_signature`.
 * Parts: [tag, id, current_root, new_root, ...account_addresses, payload]
 */
export function advanceDigest(
  id: Uint8Array,
  currentRoot: Uint8Array,
  newRoot: Uint8Array,
  accountAddresses: Uint8Array[],
  payload: Uint8Array
): Uint8Array {
  assert32(id, "id");
  assert32(currentRoot, "currentRoot");
  assert32(newRoot, "newRoot");
  for (let i = 0; i < accountAddresses.length; i++) {
    assert32(accountAddresses[i], `accountAddresses[${i}]`);
  }

  const totalLen =
    WINTERWALLET_ADVANCE_TAG.length +
    32 +
    32 +
    32 +
    accountAddresses.length * 32 +
    payload.length;

  const buf = new Uint8Array(totalLen);
  let off = 0;

  buf.set(WINTERWALLET_ADVANCE_TAG, off);
  off += WINTERWALLET_ADVANCE_TAG.length;
  buf.set(id, off);
  off += 32;
  buf.set(currentRoot, off);
  off += 32;
  buf.set(newRoot, off);
  off += 32;
  for (const addr of accountAddresses) {
    buf.set(addr, off);
    off += 32;
  }
  buf.set(payload, off);

  return sha256(buf);
}

// ── Plans / Client ────────────────────────────────────────────────────

export interface AdvancePlanParams {
  walletPda: PublicKey;
  newRoot: Uint8Array;
  innerInstructions: TransactionInstruction[];
}

export class AdvancePlan {
  readonly walletPda: PublicKey;
  readonly newRoot: WinternitzRoot;
  readonly payload: Uint8Array;
  readonly accounts: AccountMeta[];
  readonly accountAddresses: Uint8Array[];
  readonly innerInstructions: TransactionInstruction[];

  constructor(params: AdvancePlanParams) {
    assert32(params.newRoot, "newRoot");
    const { payload, accounts } = encodeAdvancePayload(params.innerInstructions);

    this.walletPda = params.walletPda;
    this.newRoot = params.newRoot.slice() as WinternitzRoot;
    this.payload = payload;
    this.accounts = accounts;
    this.accountAddresses = accounts.map((meta) => meta.pubkey.toBytes());
    this.innerInstructions = [...params.innerInstructions];
  }

  digest(walletId: Uint8Array, currentRoot: Uint8Array): Uint8Array {
    return advanceDigest(
      walletId,
      currentRoot,
      this.newRoot,
      this.accountAddresses,
      this.payload
    );
  }

  createInstruction(signatureBytes: Uint8Array): TransactionInstruction {
    return createAdvanceInstruction(
      this.walletPda,
      signatureBytes,
      this.newRoot,
      this.innerInstructions
    );
  }

  withComputeBudget(
    signatureBytes: Uint8Array,
    unitLimit = DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
    unitPriceMicroLamports = 0n
  ): TransactionInstruction[] {
    return withComputeBudget(
      [this.createInstruction(signatureBytes)],
      unitLimit,
      unitPriceMicroLamports
    );
  }

  estimateLegacyTransactionSize(
    payer: PublicKey,
    signatureBytes: Uint8Array,
    unitLimit = DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
    unitPriceMicroLamports = 0n
  ): number {
    return estimateLegacyTransactionSize(
      payer,
      this.withComputeBudget(signatureBytes, unitLimit, unitPriceMicroLamports)
    );
  }
}

export function createAdvancePlan(params: AdvancePlanParams): AdvancePlan {
  return new AdvancePlan(params);
}

export function createWithdrawPlan(params: {
  walletPda: PublicKey;
  receiver: PublicKey;
  lamports: bigint;
  newRoot: Uint8Array;
}): AdvancePlan {
  return new AdvancePlan({
    walletPda: params.walletPda,
    newRoot: params.newRoot,
    innerInstructions: [
      createWithdrawInstruction(params.walletPda, params.receiver, params.lamports),
    ],
  });
}

export class WinterWalletClient {
  constructor(readonly connection: Connection) {}

  findPda(walletId: Uint8Array): [PublicKey, number] {
    return findWinterWalletPda(walletId);
  }

  fetch(walletId: Uint8Array): Promise<WinterWalletAccount> {
    return fetchWinterWalletAccount(this.connection, walletId);
  }

  buildWithdrawPlan(params: {
    walletPda: PublicKey;
    receiver: PublicKey;
    lamports: bigint;
    newRoot: Uint8Array;
  }): AdvancePlan {
    return createWithdrawPlan(params);
  }
}

// ── Transaction Helpers ───────────────────────────────────────────────

export function createSetComputeUnitLimitInstruction(
  units: number
): TransactionInstruction {
  const data = new Uint8Array(5);
  data[0] = 0x02;
  new DataView(data.buffer).setUint32(1, units, true);
  return new TransactionInstruction({
    programId: COMPUTE_BUDGET_PROGRAM_ID,
    keys: [],
    data: Buffer.from(data),
  });
}

export function createSetComputeUnitPriceInstruction(
  microLamports: bigint
): TransactionInstruction {
  assertU64(microLamports, "microLamports");
  const data = new Uint8Array(9);
  data[0] = 0x03;
  writeU64LE(data, microLamports, 1);
  return new TransactionInstruction({
    programId: COMPUTE_BUDGET_PROGRAM_ID,
    keys: [],
    data: Buffer.from(data),
  });
}

export function withComputeBudget(
  instructions: TransactionInstruction[],
  unitLimit = DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
  unitPriceMicroLamports = 0n
): TransactionInstruction[] {
  return [
    createSetComputeUnitLimitInstruction(unitLimit),
    createSetComputeUnitPriceInstruction(unitPriceMicroLamports),
    ...instructions,
  ];
}

export function estimateLegacyTransactionSize(
  payer: PublicKey,
  instructions: TransactionInstruction[]
): number {
  const accounts = collectLegacyAccounts(payer, instructions);
  const requiredSignatures = accounts.filter((account) => account.isSigner).length;
  let messageSize = 3;
  messageSize += compactU16Len(accounts.length);
  messageSize += accounts.length * 32;
  messageSize += 32;
  messageSize += compactU16Len(instructions.length);

  for (const ix of instructions) {
    messageSize += 1;
    messageSize += compactU16Len(ix.keys.length);
    messageSize += ix.keys.length;
    messageSize += compactU16Len(ix.data.length);
    messageSize += ix.data.length;
  }

  return compactU16Len(requiredSignatures) + requiredSignatures * 64 + messageSize;
}

export function assertLegacyTransactionSize(
  payer: PublicKey,
  instructions: TransactionInstruction[],
  limit = LEGACY_TRANSACTION_SIZE_LIMIT
): number {
  const estimated = estimateLegacyTransactionSize(payer, instructions);
  if (estimated > limit) {
    throw new WinterWalletError(
      `Transaction too large: ${estimated} bytes (limit ${limit})`,
      "TRANSACTION_TOO_LARGE"
    );
  }
  return estimated;
}

// ── Error ────────────────────────────────────────────────────────────

export type WinterWalletErrorCode =
  | "INVALID_ACCOUNT_DATA"
  | "ACCOUNT_NOT_FOUND"
  | "PAYLOAD_TOO_LARGE"
  | "TRANSACTION_TOO_LARGE"
  | "INVALID_AMOUNT";

export class WinterWalletError extends Error {
  constructor(
    message: string,
    public readonly code: WinterWalletErrorCode
  ) {
    super(message);
    this.name = "WinterWalletError";
  }
}

// ── Helpers ──────────────────────────────────────────────────────────

interface LegacyAccountEntry {
  pubkey: PublicKey;
  isSigner: boolean;
  isWritable: boolean;
}

function collectLegacyAccounts(
  payer: PublicKey,
  instructions: TransactionInstruction[]
): LegacyAccountEntry[] {
  const accounts: LegacyAccountEntry[] = [];
  upsertLegacyAccount(accounts, payer, true, true);

  for (const ix of instructions) {
    upsertLegacyAccount(accounts, ix.programId, false, false);
    for (const meta of ix.keys) {
      upsertLegacyAccount(accounts, meta.pubkey, meta.isSigner, meta.isWritable);
    }
  }

  accounts.sort((a, b) => legacyAccountRank(a) - legacyAccountRank(b));
  return accounts;
}

function upsertLegacyAccount(
  accounts: LegacyAccountEntry[],
  pubkey: PublicKey,
  isSigner: boolean,
  isWritable: boolean
): void {
  const existing = accounts.find((entry) => entry.pubkey.equals(pubkey));
  if (existing) {
    existing.isSigner ||= isSigner;
    existing.isWritable ||= isWritable;
    return;
  }
  accounts.push({ pubkey, isSigner, isWritable });
}

function legacyAccountRank(account: LegacyAccountEntry): number {
  if (account.isSigner && account.isWritable) return 0;
  if (account.isSigner && !account.isWritable) return 1;
  if (!account.isSigner && account.isWritable) return 2;
  return 3;
}

function compactU16Len(value: number): number {
  let len = 1;
  while (value >= 0x80) {
    value >>= 7;
    len++;
  }
  return len;
}

function writeU16LE(buf: Uint8Array, value: number, offset: number): void {
  buf[offset] = value & 0xff;
  buf[offset + 1] = (value >> 8) & 0xff;
}

function writeU64LE(buf: Uint8Array, value: bigint, offset: number): void {
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  view.setBigUint64(offset, value, true);
}

function assert32(bytes: Uint8Array, name: string): void {
  if (bytes.length !== 32) {
    throw new WinterWalletError(
      `${name} must be 32 bytes, got ${bytes.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
}

function assertLen(bytes: Uint8Array, expected: number, name: string): void {
  if (bytes.length !== expected) {
    throw new WinterWalletError(
      `${name} must be ${expected} bytes, got ${bytes.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
}

function assertU64(value: bigint, name: string): void {
  const maxU64 = (1n << 64n) - 1n;
  if (value < 0n || value > maxU64) {
    throw new WinterWalletError(
      `${name} must fit in an unsigned 64-bit integer`,
      "INVALID_AMOUNT"
    );
  }
}
