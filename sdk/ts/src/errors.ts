export type WinterWalletErrorCode =
  | "INVALID_ACCOUNT_DATA"
  | "ACCOUNT_NOT_FOUND"
  | "PAYLOAD_TOO_LARGE"
  | "TRANSACTION_TOO_LARGE";

export class WinterWalletError extends Error {
  constructor(
    message: string,
    public readonly code: WinterWalletErrorCode
  ) {
    super(message);
    this.name = "WinterWalletError";
  }
}
