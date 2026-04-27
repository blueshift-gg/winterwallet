import { WinterWalletError } from "./errors.js";

export async function sha256(data: Uint8Array): Promise<Uint8Array> {
  const hash = await globalThis.crypto.subtle.digest("SHA-256", new Uint8Array(data));
  return new Uint8Array(hash);
}

export function compactU16Len(value: number): number {
  let len = 1;
  while (value >= 0x80) {
    value >>= 7;
    len++;
  }
  return len;
}

export function assert32(bytes: Uint8Array, name: string): void {
  if (bytes.length !== 32) {
    throw new WinterWalletError(
      `${name} must be 32 bytes, got ${bytes.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
}

export function assertLen(bytes: Uint8Array, expected: number, name: string): void {
  if (bytes.length !== expected) {
    throw new WinterWalletError(
      `${name} must be ${expected} bytes, got ${bytes.length}`,
      "INVALID_ACCOUNT_DATA"
    );
  }
}
