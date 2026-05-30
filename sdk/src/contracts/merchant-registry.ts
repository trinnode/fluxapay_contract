import { NetworkProfileSwitcher, NetworkEnvironment } from "../network-profiles.js";
import { withMappedContractError } from "../index.js";

export interface MerchantRegistryConfig {
  network: NetworkEnvironment;
  rpcUrl?: string;
  contractId: string;
}

/**
 * MerchantRegistryClient provides a high-level interface for interacting with the MerchantRegistry contract.
 * Manages merchant registration, verification, and account status operations.
 */
export class MerchantRegistryClient {
  private contract: any;
  public networkSwitcher: NetworkProfileSwitcher;
  private contractId: string;
  private rpcUrl: string;
  private networkPassphrase: string;

  constructor(config: MerchantRegistryConfig) {
    this.networkSwitcher = new NetworkProfileSwitcher(config.network);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = config.rpcUrl || profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    this.contractId = config.contractId;
    this.initializeContract();
  }

  private async initializeContract(): Promise<void> {
    // Contract will be lazily initialized on first use
  }

  private getContract(): any {
    if (!this.contract) {
      const { Client } = require("@stellar/stellar-sdk/contract");
      this.contract = new Client({
        networkPassphrase: this.networkPassphrase,
        rpcUrl: this.rpcUrl,
        contractId: this.contractId,
      });
    }
    return this.contract;
  }

  /**
   * Switch the client to a different network environment.
   * @param environment - The target network environment (e.g., 'testnet', 'mainnet')
   * @param contractId - Optional contract ID for the new network
   */
  public switchNetwork(environment: NetworkEnvironment, contractId?: string): void {
    this.networkSwitcher.switchEnvironment(environment);
    const profile = this.networkSwitcher.getProfile();
    this.rpcUrl = profile.rpcUrl;
    this.networkPassphrase = profile.networkPassphrase;
    if (contractId) {
      this.contractId = contractId;
    }
    this.contract = undefined;
  }

  /**
   * Register a new merchant in the registry.
   * @param merchantId - The unique identifier for the merchant
   * @param businessName - The name of the merchant's business
   * @param settlementCurrency - The preferred settlement currency (e.g., 'USDC')
   * @returns A promise that resolves when the merchant is registered
   * @throws Error if registration fails
   */
  async registerMerchant(
    merchantId: string,
    businessName: string,
    settlementCurrency: string,
  ): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().register_merchant({
        merchant_id: merchantId,
        business_name: businessName,
        settlement_currency: settlementCurrency,
      }),
    );
  }

  /**
   * Retrieve details of a specific merchant.
   * @param merchantId - The ID of the merchant to retrieve
   * @returns A promise resolving to the merchant details
   * @throws Error if the merchant is not found
   */
  async getMerchant(merchantId: string): Promise<any> {
    return withMappedContractError(() =>
      this.getContract().get_merchant({
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Update merchant details such as business name or settlement currency.
   * @param operator - The address authorized to update merchant information
   * @param merchantId - The ID of the merchant to update
   * @param businessName - The new business name (optional)
   * @param settlementCurrency - The new settlement currency (optional)
   * @returns A promise that resolves when the merchant is updated
   * @throws Error if the update fails
   */
  async updateMerchant(
    operator: string,
    merchantId: string,
    businessName?: string,
    settlementCurrency?: string,
  ): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().update_merchant({
        operator: operator,
        merchant_id: merchantId,
        business_name: businessName || undefined,
        settlement_currency: settlementCurrency || undefined,
      }),
    );
  }

  /**
   * Suspend a merchant account, preventing further transactions.
   * @param operator - The address authorized to suspend merchants
   * @param merchantId - The ID of the merchant to suspend
   * @returns A promise that resolves when the merchant is suspended
   * @throws Error if suspension fails
   */
  async suspendMerchant(operator: string, merchantId: string): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().suspend_merchant({
        operator: operator,
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Reinstate a suspended merchant account.
   * @param operator - The address authorized to reinstate merchants
   * @param merchantId - The ID of the merchant to reinstate
   * @returns A promise that resolves when the merchant is reinstated
   * @throws Error if reinstatement fails
   */
  async reinstateMerchant(operator: string, merchantId: string): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().reinstate_merchant({
        operator: operator,
        merchant_id: merchantId,
      }),
    );
  }

  /**
   * Verify a merchant's KYC status, enabling higher transaction limits.
   * @param operator - The address authorized to verify merchants
   * @param merchantId - The ID of the merchant to verify
   * @returns A promise that resolves when the merchant is verified
   * @throws Error if verification fails
   */
  async verifyMerchant(operator: string, merchantId: string): Promise<void> {
    return withMappedContractError(() =>
      this.getContract().verify_merchant({
        operator: operator,
        merchant_id: merchantId,
      }),
    );
  }
}
