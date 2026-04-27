import {
  getStructEncoder,
  getU32Encoder,
  getU64Encoder,
  getU8Encoder,
  transformEncoder,
} from "@solana/codecs";
import {
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
