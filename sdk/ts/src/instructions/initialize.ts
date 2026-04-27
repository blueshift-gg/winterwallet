import {
  fixEncoderSize,
  getBytesEncoder,
  getStructEncoder,
  getU8Encoder,
  transformEncoder,
} from "@solana/codecs";
import {
  PublicKey,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  INITIALIZE_DISCRIMINATOR,
  SIGNATURE_LEN,
  WINTERWALLET_INITIALIZE_TAG,
  WINTERWALLET_PROGRAM_ID,
} from "../constants.js";
import { assert32, assertLen, sha256 } from "../helpers.js";

interface InitializeInstructionData {
  signature: Uint8Array;
  nextRoot: Uint8Array;
}

const initializeInstructionDataEncoder = transformEncoder(
  getStructEncoder([
    ["discriminator", getU8Encoder()],
    ["signature", fixEncoderSize(getBytesEncoder(), SIGNATURE_LEN)],
    ["nextRoot", fixEncoderSize(getBytesEncoder(), 32)],
  ]),
  (value: InitializeInstructionData) => ({
    ...value,
    discriminator: INITIALIZE_DISCRIMINATOR,
  })
);

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

  const data = initializeInstructionDataEncoder.encode({
    signature: signatureBytes,
    nextRoot,
  });

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
 * Compute the Initialize preimage digest (SHA-256).
 *
 * The program signs over just the domain tag. The wallet ID is recovered
 * from the signature, not committed in the preimage.
 */
export function initializeDigest(): Promise<Uint8Array> {
  return sha256(WINTERWALLET_INITIALIZE_TAG);
}
