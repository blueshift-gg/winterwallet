import {
  getStructEncoder,
  getU64Encoder,
  getU8Encoder,
  transformEncoder,
} from "@solana/codecs";
import {
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";
import { WINTERWALLET_PROGRAM_ID, WITHDRAW_DISCRIMINATOR } from "../constants.js";

interface WithdrawInstructionData {
  lamports: bigint;
}

const withdrawInstructionDataEncoder = transformEncoder(
  getStructEncoder([
    ["discriminator", getU8Encoder()],
    ["lamports", getU64Encoder()],
  ]),
  (value: WithdrawInstructionData) => ({
    ...value,
    discriminator: WITHDRAW_DISCRIMINATOR,
  })
);

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
  const data = withdrawInstructionDataEncoder.encode({ lamports });

  return new TransactionInstruction({
    programId: WINTERWALLET_PROGRAM_ID,
    keys: [
      { pubkey: walletPda, isSigner: false, isWritable: true },
      { pubkey: receiver, isSigner: false, isWritable: true },
    ],
    data: Buffer.from(data),
  });
}
