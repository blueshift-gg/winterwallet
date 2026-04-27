export type WinterWalletErrorCode =
  | "INVALID_ACCOUNT_DATA"
  | "ACCOUNT_NOT_FOUND"
  | "PAYLOAD_TOO_LARGE"
  | "TRANSACTION_TOO_LARGE"
  | "ROOT_MISMATCH"
  | "SIGNER_POSITION_MISMATCH"
  | "POSITION_OVERFLOW"
  | "UNSUPPORTED_TRANSACTION";

export class WinterWalletError extends Error {
  constructor(
    message: string,
    public readonly code: WinterWalletErrorCode
  ) {
    super(message);
    this.name = "WinterWalletError";
  }
}
