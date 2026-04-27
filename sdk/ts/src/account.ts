import {
  fixDecoderSize,
  getBytesDecoder,
  getStructDecoder,
  getU8Decoder,
} from "@solana/codecs";
import { Connection, PublicKey } from "@solana/web3.js";
import {
  WALLET_ACCOUNT_LEN,
  WINTERWALLET_PROGRAM_ID,
  WINTERWALLET_SEED,
} from "./constants.js";
import { WinterWalletError } from "./errors.js";
import { assert32 } from "./helpers.js";

/** 32-byte wallet identifier (merklized Winternitz pubkey root). */
export type WalletId = Uint8Array & { readonly __brand: "WalletId" };

/** 32-byte Winternitz merkle root. */
export type WinternitzRoot = Uint8Array & {
  readonly __brand: "WinternitzRoot";
};

export interface WinterWalletAccount {
  readonly id: WalletId;
  readonly root: WinternitzRoot;
  readonly bump: number;
}

const winterWalletAccountDecoder = getStructDecoder([
  ["id", fixDecoderSize(getBytesDecoder(), 32)],
  ["root", fixDecoderSize(getBytesDecoder(), 32)],
  ["bump", getU8Decoder()],
]);

export function deserializeWinterWalletAccount(
  data: Uint8Array
): WinterWalletAccount {
  if (data.length !== WALLET_ACCOUNT_LEN) {
    throw new WinterWalletError(
      `Account data must be exactly ${WALLET_ACCOUNT_LEN} bytes, got ${data.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
  const decoded = winterWalletAccountDecoder.decode(data);
  return {
    id: new Uint8Array(decoded.id) as WalletId,
    root: new Uint8Array(decoded.root) as WinternitzRoot,
    bump: decoded.bump,
  };
}

export function findWinterWalletPda(
  walletId: Uint8Array
): [PublicKey, number] {
  assert32(walletId, "walletId");
  return PublicKey.findProgramAddressSync(
    [WINTERWALLET_SEED, walletId],
    WINTERWALLET_PROGRAM_ID
  );
}

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
  return deserializeWinterWalletAccount(info.data);
}
