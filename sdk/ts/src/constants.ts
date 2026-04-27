import { PublicKey } from "@solana/web3.js";

export const WINTERWALLET_PROGRAM_ID = new PublicKey(
  "winter5vMwvf51xrSVPTxbAAD6qiSTmPeRTSizMCQCa"
);

export const INITIALIZE_DISCRIMINATOR = 0;
export const ADVANCE_DISCRIMINATOR = 1;
export const WITHDRAW_DISCRIMINATOR = 2;
export const CLOSE_DISCRIMINATOR = 3;

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
