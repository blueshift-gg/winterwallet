import { describe, it, expect } from "vitest";
import {
  SIGNATURE_LEN,
  WALLET_ACCOUNT_LEN,
  WINTERWALLET_PROGRAM_ID,
  createAdvanceInstruction,
  createInitializeInstruction,
  createWithdrawInstruction,
  deserializeWinterWalletAccount,
  encodeAdvancePayload,
  findWinterWalletPda,
  advanceDigest,
  initializeDigest,
  WinterWalletError,
  WinterWalletClient,
  assertLegacyTransactionSize,
  createWithdrawPlan,
  estimateLegacyTransactionSize,
  withComputeBudget,
} from "../src/index.js";
import { PublicKey, TransactionInstruction } from "@solana/web3.js";

describe("deserializeWinterWalletAccount", () => {
  it("deserializes valid 65-byte data", () => {
    const data = new Uint8Array(65);
    data.fill(0x01, 0, 32); // id
    data.fill(0x02, 32, 64); // root
    data[64] = 42; // bump

    const account = deserializeWinterWalletAccount(data);
    expect(account.id).toEqual(new Uint8Array(32).fill(0x01));
    expect(account.root).toEqual(new Uint8Array(32).fill(0x02));
    expect(account.bump).toBe(42);
  });

  it("rejects data shorter than 65 bytes", () => {
    expect(() => deserializeWinterWalletAccount(new Uint8Array(64))).toThrow(
      WinterWalletError
    );
  });

  it("rejects data longer than 65 bytes", () => {
    expect(() => deserializeWinterWalletAccount(new Uint8Array(66))).toThrow(
      WinterWalletError
    );
  });
});

describe("findWinterWalletPda", () => {
  it("is deterministic", () => {
    const id = new Uint8Array(32).fill(0xaa);
    const [addr1, bump1] = findWinterWalletPda(id);
    const [addr2, bump2] = findWinterWalletPda(id);
    expect(addr1.equals(addr2)).toBe(true);
    expect(bump1).toBe(bump2);
  });

  it("different IDs produce different addresses", () => {
    const [a] = findWinterWalletPda(new Uint8Array(32).fill(0x01));
    const [b] = findWinterWalletPda(new Uint8Array(32).fill(0x02));
    expect(a.equals(b)).toBe(false);
  });

  it("rejects non-32-byte input", () => {
    expect(() => findWinterWalletPda(new Uint8Array(16))).toThrow(
      WinterWalletError
    );
  });
});

describe("createInitializeInstruction", () => {
  it("produces correct data layout", () => {
    const payer = PublicKey.unique();
    const walletPda = PublicKey.unique();
    const sig = new Uint8Array(SIGNATURE_LEN).fill(0xab);
    const root = new Uint8Array(32).fill(0xcd);

    const ix = createInitializeInstruction(payer, walletPda, sig, root);

    expect(ix.programId.equals(WINTERWALLET_PROGRAM_ID)).toBe(true);
    expect(ix.keys.length).toBe(3);
    expect(ix.keys[0].isSigner).toBe(true); // payer
    expect(ix.keys[1].isSigner).toBe(false); // wallet PDA
    expect(ix.data.length).toBe(1 + SIGNATURE_LEN + 32);
    expect(ix.data[0]).toBe(0); // INITIALIZE discriminator
  });

  it("rejects wrong signature length", () => {
    expect(() =>
      createInitializeInstruction(
        PublicKey.unique(),
        PublicKey.unique(),
        new Uint8Array(64), // wrong
        new Uint8Array(32)
      )
    ).toThrow(WinterWalletError);
  });

  it("rejects wrong root length", () => {
    expect(() =>
      createInitializeInstruction(
        PublicKey.unique(),
        PublicKey.unique(),
        new Uint8Array(SIGNATURE_LEN),
        new Uint8Array(16) // wrong
      )
    ).toThrow(WinterWalletError);
  });
});

describe("createWithdrawInstruction", () => {
  it("produces correct data layout with bigint lamports", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const lamports = 1_000_000_000n;

    const ix = createWithdrawInstruction(walletPda, receiver, lamports);

    expect(ix.data.length).toBe(9); // disc(1) + u64(8)
    expect(ix.data[0]).toBe(2); // WITHDRAW discriminator
    expect(ix.keys.length).toBe(2);
    expect(ix.keys[0].isSigner).toBe(false); // PDA signer is promoted on-chain
    expect(ix.keys[0].isWritable).toBe(true);
  });

  it("rejects negative lamports", () => {
    expect(() =>
      createWithdrawInstruction(PublicKey.unique(), PublicKey.unique(), -1n)
    ).toThrow(WinterWalletError);
  });
});

describe("encodeAdvancePayload", () => {
  it("scrubs signer flags on passthrough accounts", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const inner = createWithdrawInstruction(walletPda, receiver, 1000n);

    const { accounts } = encodeAdvancePayload([inner]);

    // All passthrough accounts should have isSigner=false
    for (const meta of accounts) {
      expect(meta.isSigner).toBe(false);
    }
    // wallet_pda should still be writable
    expect(accounts[1].isWritable).toBe(true);
  });

  it("validates MAX_PASSTHROUGH_ACCOUNTS", () => {
    // Create an instruction with many accounts
    const keys = Array.from({ length: 130 }, () => ({
      pubkey: PublicKey.unique(),
      isSigner: false,
      isWritable: false,
    }));
    const ix = new TransactionInstruction({
      programId: PublicKey.unique(),
      keys,
      data: Buffer.from([]),
    });

    expect(() => encodeAdvancePayload([ix])).toThrow(WinterWalletError);
  });

  it("validates MAX_CPI_INSTRUCTION_ACCOUNTS", () => {
    const keys = Array.from({ length: 17 }, () => ({
      pubkey: PublicKey.unique(),
      isSigner: false,
      isWritable: false,
    }));
    const ix = new TransactionInstruction({
      programId: PublicKey.unique(),
      keys,
      data: Buffer.from([]),
    });

    expect(() => encodeAdvancePayload([ix])).toThrow(WinterWalletError);
  });

  it("encodes payload wire format correctly", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const inner = createWithdrawInstruction(walletPda, receiver, 500_000n);

    const { payload } = encodeAdvancePayload([inner]);

    expect(payload[0]).toBe(1); // 1 inner instruction
    expect(payload[1]).toBe(2); // 2 accounts
    const dataLen = payload[2] | (payload[3] << 8);
    expect(dataLen).toBe(9); // disc(1) + lamports(8)
  });
});

describe("createAdvanceInstruction", () => {
  it("wallet PDA is not marked as signer in outer instruction", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const sig = new Uint8Array(SIGNATURE_LEN);
    const root = new Uint8Array(32);
    const inner = createWithdrawInstruction(walletPda, receiver, 1000n);

    const ix = createAdvanceInstruction(walletPda, sig, root, [inner]);

    // First key is wallet PDA — must NOT be signer in outer tx
    expect(ix.keys[0].isSigner).toBe(false);
    expect(ix.keys[0].isWritable).toBe(true);

    // No account should be marked as signer
    for (const key of ix.keys) {
      expect(key.isSigner).toBe(false);
    }
  });
});

describe("advanceDigest", () => {
  it("validates 32-byte inputs", () => {
    expect(() =>
      advanceDigest(
        new Uint8Array(16), // wrong
        new Uint8Array(32),
        new Uint8Array(32),
        [],
        new Uint8Array(0)
      )
    ).toThrow(WinterWalletError);
  });

  it("validates account address lengths", () => {
    expect(() =>
      advanceDigest(
        new Uint8Array(32),
        new Uint8Array(32),
        new Uint8Array(32),
        [new Uint8Array(16)], // wrong
        new Uint8Array(0)
      )
    ).toThrow(WinterWalletError);
  });

  it("produces deterministic 32-byte digest", () => {
    const id = new Uint8Array(32).fill(1);
    const cur = new Uint8Array(32).fill(2);
    const next = new Uint8Array(32).fill(3);

    const d1 = advanceDigest(id, cur, next, [], new Uint8Array([0]));
    const d2 = advanceDigest(id, cur, next, [], new Uint8Array([0]));

    expect(d1.length).toBe(32);
    expect(d1).toEqual(d2);
  });
});

describe("initializeDigest", () => {
  it("returns 32-byte digest", () => {
    const digest = initializeDigest();
    expect(digest.length).toBe(32);
  });
});

describe("AdvancePlan", () => {
  it("creates digest and instruction from one account order", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const newRoot = new Uint8Array(32).fill(3);
    const plan = createWithdrawPlan({
      walletPda,
      receiver,
      lamports: 123n,
      newRoot,
    });

    const digest = plan.digest(
      new Uint8Array(32).fill(1),
      new Uint8Array(32).fill(2)
    );
    const ix = plan.createInstruction(new Uint8Array(SIGNATURE_LEN));

    expect(digest.length).toBe(32);
    expect(ix.keys[0].pubkey.equals(walletPda)).toBe(true);
    expect(plan.payload[0]).toBe(1);
    expect(plan.accountAddresses.length).toBe(plan.accounts.length);
  });

  it("estimates legacy transaction size with compute budget", () => {
    const walletPda = PublicKey.unique();
    const receiver = PublicKey.unique();
    const payer = PublicKey.unique();
    const plan = createWithdrawPlan({
      walletPda,
      receiver,
      lamports: 123n,
      newRoot: new Uint8Array(32),
    });

    const size = plan.estimateLegacyTransactionSize(
      payer,
      new Uint8Array(SIGNATURE_LEN)
    );

    expect(size).toBeGreaterThan(0);
    expect(size).toBeLessThan(1232);
  });
});

describe("WinterWalletClient", () => {
  it("builds withdraw plans without fetching network state", () => {
    const client = new WinterWalletClient({} as never);
    const plan = client.buildWithdrawPlan({
      walletPda: PublicKey.unique(),
      receiver: PublicKey.unique(),
      lamports: 1n,
      newRoot: new Uint8Array(32),
    });

    expect(plan.payload[0]).toBe(1);
  });
});

describe("transaction helpers", () => {
  it("prefixes compute budget instructions and estimates size", () => {
    const payer = PublicKey.unique();
    const ix = createWithdrawInstruction(PublicKey.unique(), PublicKey.unique(), 1n);
    const instructions = withComputeBudget([ix]);
    const size = estimateLegacyTransactionSize(payer, instructions);

    expect(instructions.length).toBe(3);
    expect(size).toBeGreaterThan(0);
    expect(assertLegacyTransactionSize(payer, instructions)).toBe(size);
  });
});
