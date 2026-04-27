import {
  addEncoderSizePrefix,
  fixEncoderSize,
  getArrayEncoder,
  getBytesEncoder,
  getStructEncoder,
  getU16Encoder,
  getU8Encoder,
  transformEncoder,
} from "@solana/codecs";
import {
  PublicKey,
  TransactionInstruction,
  type AccountMeta,
} from "@solana/web3.js";
import {
  ADVANCE_DISCRIMINATOR,
  MAX_CPI_INSTRUCTION_ACCOUNTS,
  MAX_PASSTHROUGH_ACCOUNTS,
  SIGNATURE_LEN,
  WINTERWALLET_ADVANCE_TAG,
  WINTERWALLET_PROGRAM_ID,
} from "../constants.js";
import { WinterWalletError } from "../errors.js";
import { assert32, assertLen, sha256 } from "../helpers.js";

interface InnerInstructionData {
  numAccounts: number;
  data: Uint8Array;
}

const innerInstructionEncoder = getStructEncoder([
  ["numAccounts", getU8Encoder()],
  ["data", addEncoderSizePrefix(getBytesEncoder(), getU16Encoder())],
]);

const advancePayloadEncoder = getArrayEncoder(innerInstructionEncoder, {
  size: getU8Encoder(),
});

interface AdvanceInstructionData {
  signature: Uint8Array;
  newRoot: Uint8Array;
  payload: Uint8Array;
}

const advanceInstructionDataEncoder = transformEncoder(
  getStructEncoder([
    ["discriminator", getU8Encoder()],
    ["signature", fixEncoderSize(getBytesEncoder(), SIGNATURE_LEN)],
    ["newRoot", fixEncoderSize(getBytesEncoder(), 32)],
    ["payload", getBytesEncoder()],
  ]),
  (value: AdvanceInstructionData) => ({
    ...value,
    discriminator: ADVANCE_DISCRIMINATOR,
  })
);

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

  const data = advanceInstructionDataEncoder.encode({
    signature: signatureBytes,
    newRoot,
    payload,
  });

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

export interface AdvancePayloadResult {
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

  const accounts: AccountMeta[] = [];
  const innerData: InnerInstructionData[] = [];

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

    accounts.push({
      pubkey: ix.programId,
      isSigner: false,
      isWritable: false,
    });
    for (const meta of ix.keys) {
      accounts.push({
        pubkey: meta.pubkey,
        isSigner: false,
        isWritable: meta.isWritable,
      });
    }
    innerData.push({ numAccounts: ix.keys.length, data: ix.data });
  }

  const payload = new Uint8Array(advancePayloadEncoder.encode(innerData));
  return { payload, accounts };
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
): Promise<Uint8Array> {
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
