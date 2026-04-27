import {
  getStructEncoder,
  getU32Encoder,
  getU64Encoder,
  getU8Encoder,
  transformEncoder,
} from "@solana/codecs";
import {
  AddressLookupTableAccount,
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  COMPUTE_BUDGET_PROGRAM_ID,
  DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT,
  LEGACY_TRANSACTION_SIZE_LIMIT,
} from "./constants.js";
import { WinterWalletError } from "./errors.js";
import { compactU16Len } from "./helpers.js";

const SET_COMPUTE_UNIT_LIMIT_DISCRIMINATOR = 0x02;
const SET_COMPUTE_UNIT_PRICE_DISCRIMINATOR = 0x03;

const setComputeUnitLimitDataEncoder = transformEncoder(
  getStructEncoder([
    ["discriminator", getU8Encoder()],
    ["units", getU32Encoder()],
  ]),
  (value: { units: number }) => ({
    ...value,
    discriminator: SET_COMPUTE_UNIT_LIMIT_DISCRIMINATOR,
  })
);

const setComputeUnitPriceDataEncoder = transformEncoder(
  getStructEncoder([
    ["discriminator", getU8Encoder()],
    ["microLamports", getU64Encoder()],
  ]),
  (value: { microLamports: bigint }) => ({
    ...value,
    discriminator: SET_COMPUTE_UNIT_PRICE_DISCRIMINATOR,
  })
);

export function createSetComputeUnitLimitInstruction(
  units: number
): TransactionInstruction {
  return new TransactionInstruction({
    programId: COMPUTE_BUDGET_PROGRAM_ID,
    keys: [],
    data: Buffer.from(setComputeUnitLimitDataEncoder.encode({ units })),
  });
}

export function createSetComputeUnitPriceInstruction(
  microLamports: bigint
): TransactionInstruction {
  return new TransactionInstruction({
    programId: COMPUTE_BUDGET_PROGRAM_ID,
    keys: [],
    data: Buffer.from(setComputeUnitPriceDataEncoder.encode({ microLamports })),
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

// ── V0 (versioned) transaction support ──────────────────────────────
//
// Note: v0 estimation is TS-only. Rust client deferred until needed.
//
// Security: ALT account substitution is safe because the Advance
// preimage binds raw 32-byte account addresses into the Winternitz
// digest. A malicious ALT causes signature verification failure,
// not silent wrong execution. Do not remove address binding from
// the preimage.

interface V0Lookup {
  tableKey: PublicKey;
  writableIndexes: number[];
  readonlyIndexes: number[];
}

interface V0Partition {
  staticKeys: LegacyAccountEntry[];
  lookups: V0Lookup[];
}

/**
 * Estimate the wire size of a v0 transaction with address lookup tables.
 *
 * Accounts found in the provided ALTs are referenced by 1-byte index
 * instead of 32-byte pubkey, reducing message size. Signers and invoked
 * programs always remain in static keys per Solana v0 spec.
 */
export function estimateV0TransactionSize(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[]
): number {
  const { staticKeys, lookups } = partitionV0Accounts(
    payer,
    instructions,
    addressLookupTableAccounts
  );

  const requiredSignatures = staticKeys.filter((a) => a.isSigner).length;

  // V0 message size.
  let messageSize = 1; // version prefix (0x80)
  messageSize += 3; // header (numSigners, numReadonlySigned, numReadonlyUnsigned)
  messageSize += compactU16Len(staticKeys.length);
  messageSize += staticKeys.length * 32;
  messageSize += 32; // recent blockhash
  messageSize += compactU16Len(instructions.length);

  // Total account count for index resolution: static + all lookup entries.
  for (const ix of instructions) {
    messageSize += 1; // program ID index
    messageSize += compactU16Len(ix.keys.length);
    messageSize += ix.keys.length; // 1-byte account indices
    messageSize += compactU16Len(ix.data.length);
    messageSize += ix.data.length;
  }

  // Address table lookups section.
  messageSize += compactU16Len(lookups.length);
  for (const lookup of lookups) {
    messageSize += 32; // ALT address
    messageSize += compactU16Len(lookup.writableIndexes.length);
    messageSize += lookup.writableIndexes.length;
    messageSize += compactU16Len(lookup.readonlyIndexes.length);
    messageSize += lookup.readonlyIndexes.length;
  }

  return compactU16Len(requiredSignatures) + requiredSignatures * 64 + messageSize;
}

/**
 * Throws TRANSACTION_TOO_LARGE if the estimated v0 size exceeds the limit.
 */
export function assertV0TransactionSize(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[],
  limit = LEGACY_TRANSACTION_SIZE_LIMIT
): number {
  const estimated = estimateV0TransactionSize(
    payer,
    instructions,
    addressLookupTableAccounts
  );
  if (estimated > limit) {
    throw new WinterWalletError(
      `Transaction too large: ${estimated} bytes (limit ${limit})`,
      "TRANSACTION_TOO_LARGE"
    );
  }
  return estimated;
}

/**
 * Partition accounts into static keys and ALT lookups.
 *
 * Rules (per Solana v0 spec):
 * - Signers always stay in static keys (ALTs cannot grant signer status)
 * - Invoked programs (programId) always stay in static keys
 * - Non-signer, non-program accounts found in an ALT go to lookup indices
 * - First ALT match wins when an account appears in multiple ALTs
 */
function partitionV0Accounts(
  payer: PublicKey,
  instructions: readonly TransactionInstruction[],
  addressLookupTableAccounts: readonly AddressLookupTableAccount[]
): V0Partition {
  // Collect all accounts with their flags (same as legacy).
  const accounts = collectLegacyAccounts(payer, instructions as TransactionInstruction[]);

  // Build a set of invoked program IDs.
  const invokedPrograms = new Set<string>();
  for (const ix of instructions) {
    invokedPrograms.add(ix.programId.toBase58());
  }

  // Build a Map from base58 → { tableIndex, entryIndex } for O(1) ALT lookups.
  const altMap = new Map<string, { tableIndex: number; entryIndex: number }>();
  for (let t = 0; t < addressLookupTableAccounts.length; t++) {
    const addresses = addressLookupTableAccounts[t].state.addresses;
    for (let e = 0; e < addresses.length; e++) {
      const key = addresses[e].toBase58();
      if (!altMap.has(key)) {
        altMap.set(key, { tableIndex: t, entryIndex: e });
      }
    }
  }

  // Partition accounts.
  const staticKeys: LegacyAccountEntry[] = [];
  const lookupBuilders: Map<number, { writable: number[]; readonly: number[] }> = new Map();

  for (const account of accounts) {
    const key58 = account.pubkey.toBase58();

    // Signers and invoked programs must stay in static keys.
    if (account.isSigner || invokedPrograms.has(key58)) {
      staticKeys.push(account);
      continue;
    }

    const alt = altMap.get(key58);
    if (alt === undefined) {
      staticKeys.push(account);
      continue;
    }

    // Place into the appropriate ALT lookup bucket.
    let builder = lookupBuilders.get(alt.tableIndex);
    if (!builder) {
      builder = { writable: [], readonly: [] };
      lookupBuilders.set(alt.tableIndex, builder);
    }
    if (account.isWritable) {
      builder.writable.push(alt.entryIndex);
    } else {
      builder.readonly.push(alt.entryIndex);
    }
  }

  // Build lookup entries (only for ALTs that actually resolved accounts).
  const lookups: V0Lookup[] = [];
  for (const [tableIndex, builder] of lookupBuilders) {
    lookups.push({
      tableKey: addressLookupTableAccounts[tableIndex].key,
      writableIndexes: builder.writable,
      readonlyIndexes: builder.readonly,
    });
  }

  return { staticKeys, lookups };
}
