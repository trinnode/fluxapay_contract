import {
  Client as ContractClient,
  Merchant,
  PaymentCharge,
  Refund,
  Dispute,
  PaymentStatus,
  RefundStatus,
  DisputeStatus,
} from "./contracts/fluxapay/src/index.js";
import { Networks } from "@stellar/stellar-sdk";
import {
  FluxapayOfflineSigner,
  OfflineTransactionPayload,
  buildOfflinePayload,
  buildCreatePaymentPayload,
  buildVerifyPaymentPayload,
  buildCreateRefundPayload,
  prepareForOfflineSigning,
  restoreFromOfflinePayload,
} from "./offline-signer.js";
import { NetworkProfileSwitcher, NetworkEnvironment, NetworkProfiles, NetworkProfile } from "./network-profiles.js";

export interface FluxapayConfig {
  network: NetworkEnvironment;
  rpcUrl?: string;
  contractId: string;
}

export const FLUXAPAY_CONTRACT_ERROR_MAP: Record<number, string> = {
  1: "Unauthorized",
  2: "PaymentAlreadyExists",
  3: "PaymentExpired",
  4: "InvalidPaymentId",
  8: "RefundAlreadyProcessed",
  9: "DisputeNotFound",
  12: "DisputeAlreadyResolved",
  14: "PaymentAlreadyProcessed",
  15: "AccessControlError",
  16: "RefundExceedsPayment",
  17: "ContractPaused",
  18: "RateLimitExceeded",
  19: "RefundCancelled",
  20: "UnsupportedToken",
  21: "AmountBelowMin",
  22: "AmountAboveMax",
  23: "InvalidExpiry",
  24: "InvalidSettlement",
  25: "DuplicateIdempotencyKey",
  26: "InvalidAddress",
  27: "ArbitrageDetected",
  28: "SwapPathInvalid",
  29: "OraclePriceDeviation",
  30: "SubscriptionInGracePeriod",
  31: "SubscriptionRetryExhausted",
  32: "InvalidResumeTimestamp",
  33: "MerchantAuthError",
  34: "TierVolumeLimitExceeded",
  35: "RefundExpired",
  36: "InsufficientArbitrators",
  37: "ArbitrationVotingThresholdNotMet",
  38: "FeeProposalNotReady",
  39: "NoFeeProposal",
  404: "PaymentNotFound",
  405: "RefundNotFound",
  406: "InvalidAmount",
};

export class FluxapayError extends Error {
  readonly code: number;
  readonly contractErrorName: string;
  readonly cause?: unknown;

  constructor(code: number, contractErrorName: string, message?: string, cause?: unknown) {
    super(message ?? contractErrorName);
    this.name = `${contractErrorName}Error`;
    this.code = code;
    this.contractErrorName = contractErrorName;
    this.cause = cause;
  }
}

const HOST_ERROR_CODE_REGEX = /Error\(Contract,\s*#(\d+)\)/;

function parseContractErrorCode(error: unknown): number | null {
  if (typeof error !== "object" || error === null) {
    return null;
  }

  const maybeCode = (error as { code?: unknown }).code;
  if (typeof maybeCode === "number") {
    return maybeCode;
  }

  const maybeMessage = (error as { message?: unknown }).message;
  if (typeof maybeMessage === "string") {
    const match = maybeMessage.match(HOST_ERROR_CODE_REGEX);
    if (match && match[1]) {
      return Number(match[1]);
    }
  }

  const maybeResult = (error as { result?: unknown }).result;
  if (typeof maybeResult === "string") {
    const match = maybeResult.match(HOST_ERROR_CODE_REGEX);
    if (match && match[1]) {
      return Number(match[1]);
    }
  }

  return null;
}

function toFluxapayError(error: unknown): FluxapayError {
  const code = parseContractErrorCode(error);
  if (code === null) {
    if (error instanceof Error) {
      throw error;
    }
    throw new Error("Unknown Fluxapay SDK error");
  }

  const contractErrorName = FLUXAPAY_CONTRACT_ERROR_MAP[code] ?? "UnknownContractError";
  return new FluxapayError(
    code,
    contractErrorName,
    `${contractErrorName} (contract error #${code})`,
    error,
  );
}

async function withMappedContractError<T>(operation: () => Promise<T>): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    throw toFluxapayError(error);
  }
}

export class FluxapayClient {
  public contract: ContractClient;
  public networkSwitcher: NetworkProfileSwitcher;

  constructor(config: FluxapayConfig) {
    this.networkSwitcher = new NetworkProfileSwitcher(config.network);
    
    // Override RPC URL if provided, otherwise use the default for the profile
    const rpcUrl = config.rpcUrl || this.networkSwitcher.getProfile().rpcUrl;
    
    this.contract = new ContractClient({
      networkPassphrase: this.networkSwitcher.getProfile().networkPassphrase,
      rpcUrl: rpcUrl,
      contractId: config.contractId,
    });
  }

  /**
   * Switch the client to a different network environment.
   * This re-initializes the contract client seamlessly.
   */
  public switchNetwork(environment: NetworkEnvironment, contractId?: string): void {
    this.networkSwitcher.switchEnvironment(environment);
    const profile = this.networkSwitcher.getProfile();
    const newContractId = contractId || profile.defaultContractId || this.contract.options.contractId;
    
    this.contract = new ContractClient({
      networkPassphrase: profile.networkPassphrase,
      rpcUrl: profile.rpcUrl,
      contractId: newContractId,
    });
  }

  /**
   * Create a new payment charge
   */
  async createPayment(params: {
    paymentId: string;
    merchantId: string;
    amount: bigint;
    currency: string;
    depositAddress: string;
    expiresAt: bigint;
  }) {
    return withMappedContractError(() =>
      this.contract.create_payment({
        payment_id: params.paymentId,
        merchant_id: params.merchantId,
        amount: params.amount,
        currency: params.currency,
        deposit_address: params.depositAddress,
        expires_at: params.expiresAt,
      }),
    );
  }

  /**
   * Verify a payment via oracle
   */
  async verifyPayment(params: {
    oracle: string;
    paymentId: string;
    transactionHash: Buffer;
    payerAddress: string;
    amountReceived: bigint;
  }) {
    return withMappedContractError(() =>
      this.contract.verify_payment({
        oracle: params.oracle,
        payment_id: params.paymentId,
        transaction_hash: params.transactionHash,
        payer_address: params.payerAddress,
        amount_received: params.amountReceived,
      }),
    );
  }

  /**
   * Create a refund request
   */
  async createRefund(params: {
    paymentId: string;
    amount: bigint;
    reason: string;
    requester: string;
  }) {
    return withMappedContractError(() =>
      this.contract.create_refund({
        payment_id: params.paymentId,
        refund_amount: params.amount,
        reason: params.reason,
        requester: params.requester,
      }),
    );
  }

  /**
   * Get merchant details
   */
  async getMerchant(merchantId: string) {
    return withMappedContractError(() =>
      this.contract.get_merchant({
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Get payment details
   */
  async getPayment(paymentId: string) {
    return withMappedContractError(() =>
      this.contract.get_payment({ payment_id: paymentId }),
    );
  }

  /** Offline/hardware wallet payload builder utilities. */
  offlineSigner(): FluxapayOfflineSigner {
    return new FluxapayOfflineSigner(
      this.contract as import("./offline-signer.js").OfflineCapableClient,
      this.contract.options.contractId,
      this.contract.options.networkPassphrase,
    );
  }
}

export { toFluxapayError, withMappedContractError };

export {
  Merchant,
  PaymentCharge,
  Refund,
  Dispute,
  PaymentStatus,
  RefundStatus,
  DisputeStatus,
  FluxapayOfflineSigner,
  OfflineTransactionPayload,
  buildOfflinePayload,
  buildCreatePaymentPayload,
  buildVerifyPaymentPayload,
  buildCreateRefundPayload,
  prepareForOfflineSigning,
  restoreFromOfflinePayload,
  NetworkProfileSwitcher,
  NetworkEnvironment,
  NetworkProfiles,
  NetworkProfile,
};

export { RefundManagerClient, type RefundManagerConfig } from "./contracts/refund-manager.js";
export { MerchantRegistryClient, type MerchantRegistryConfig } from "./contracts/merchant-registry.js";
