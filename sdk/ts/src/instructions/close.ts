import { getStructEncoder, getU8Encoder, transformEncoder } from "@solana/codecs";
import {
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";
import { CLOSE_DISCRIMINATOR, WINTERWALLET_PROGRAM_ID } from "../constants.js";

const closeInstructionDataEncoder = transformEncoder(
  getStructEncoder([["discriminator", getU8Encoder()]]),
  () => ({ discriminator: CLOSE_DISCRIMINATOR })
);

/**
 * Build a Close instruction for use as an inner CPI inside Advance.
 *
 * Sweeps all lamports from the wallet PDA to `receiver` and tears down the
 * account: the program zeros `data_len`, lamports, and reassigns owner to
 * System. As with Withdraw, this MUST be passed to `createAdvanceInstruction`
 * — the wallet PDA is promoted to signer on-chain via `invoke_signed`.
 */
export function createCloseInstruction(
  walletPda: PublicKey,
  receiver: PublicKey
): TransactionInstruction {
  return new TransactionInstruction({
    programId: WINTERWALLET_PROGRAM_ID,
    keys: [
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: receiver, isSigner: false, isWritable: true },
    ],
    data: Buffer.from(closeInstructionDataEncoder.encode({})),
  });
}
