import {
  Connection,
  PublicKey,
  TransactionInstruction,
  type AccountMeta,
} from "@solana/web3.js";
import {
  WinterWalletAccount,
  WinternitzRoot,
  fetchWinterWalletAccount,
  findWinterWalletPda,
} from "./account.js";
import { DEFAULT_ADVANCE_COMPUTE_UNIT_LIMIT } from "./constants.js";
import {
  advanceDigest,
  createAdvanceInstruction,
  createCloseInstruction,
  createWithdrawInstruction,
  encodeAdvancePayload,
} from "./instructions/index.js";
import { assert32 } from "./helpers.js";
import {
  estimateLegacyTransactionSize,
  withComputeBudget,
} from "./transaction.js";

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

  digest(walletId: Uint8Array, currentRoot: Uint8Array): Promise<Uint8Array> {
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

export function createClosePlan(params: {
  walletPda: PublicKey;
  receiver: PublicKey;
  newRoot: Uint8Array;
}): AdvancePlan {
  return new AdvancePlan({
    walletPda: params.walletPda,
    newRoot: params.newRoot,
    innerInstructions: [createCloseInstruction(params.walletPda, params.receiver)],
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

  buildClosePlan(params: {
    walletPda: PublicKey;
    receiver: PublicKey;
    newRoot: Uint8Array;
  }): AdvancePlan {
    return createClosePlan(params);
  }
}
