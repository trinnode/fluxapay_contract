# @fluxapay/sdk

Official TypeScript SDK for interacting with FluxaPay's Soroban smart contracts on the Stellar network.

## Installation

```bash
npm install @fluxapay/sdk
```

## Quick Start

```typescript
import { FluxapayClient } from "@fluxapay/sdk";

const client = new FluxapayClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // Your contract ID
});

async function main() {
  // Create a payment
  const payment = await client.createPayment({
    paymentId: "pay_123",
    merchantId: "G...",
    amount: 1000000n, // 1 USDC
    currency: "USDC",
    depositAddress: "G...",
    expiresAt: BigInt(Math.floor(Date.now() / 1000) + 3600),
  });

  console.log("Payment created:", payment);

  // Get payment status
  const status = await client.getPayment("pay_123");
  console.log("Payment status:", status);
}
```

## Features

- **High-level Wrapper**: `FluxapayClient`, `RefundManagerClient`, and `MerchantRegistryClient` simplify complex contract interactions.
- **Typed Interfaces**: Full TypeScript support for all contract models (`Merchant`, `Payment`, `Refund`, etc.).
- **Automatic Simulation**: Built-in support for Soroban transaction simulation.
- **Network Presets**: Easy switching between `testnet` and `mainnet`.

## RefundManagerClient

The `RefundManagerClient` provides methods for managing refunds:

```typescript
import { RefundManagerClient } from "@fluxapay/sdk";

const refundClient = new RefundManagerClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // RefundManager contract ID
});

async function handleRefund() {
  // Create a refund request
  const refundId = await refundClient.createRefund(
    "payment_123",     // paymentId
    500000n,           // refundAmount in stroops
    "Damaged goods",   // reason
    "G...",            // requester address
  );

  console.log("Refund created:", refundId);

  // Get refund details
  const refund = await refundClient.getRefund(refundId);
  console.log("Refund status:", refund.status);

  // Process the refund
  await refundClient.processRefund("G...", refundId); // operator, refundId

  // Get all refunds for a payment
  const allRefunds = await refundClient.getPaymentRefunds("payment_123");
  console.log("Payment refunds:", allRefunds);
}
```

## MerchantRegistryClient

The `MerchantRegistryClient` provides methods for managing merchant registrations:

```typescript
import { MerchantRegistryClient } from "@fluxapay/sdk";

const merchantClient = new MerchantRegistryClient({
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  contractId: "C...", // MerchantRegistry contract ID
});

async function manageMerchant() {
  // Register a new merchant
  await merchantClient.registerMerchant(
    "merchant_001",      // merchantId
    "Acme Corp",         // businessName
    "USDC",              // settlementCurrency
  );

  console.log("Merchant registered");

  // Get merchant details
  const merchant = await merchantClient.getMerchant("merchant_001");
  console.log("Merchant:", merchant);

  // Verify the merchant
  await merchantClient.verifyMerchant("G...", "merchant_001"); // operator, merchantId

  // Update merchant information
  await merchantClient.updateMerchant(
    "G...",              // operator
    "merchant_001",      // merchantId
    "Updated Corp Name", // new businessName
    "EUR",               // new settlementCurrency
  );

  // Suspend a merchant if needed
  await merchantClient.suspendMerchant("G...", "merchant_001");

  // Reinstate a suspended merchant
  await merchantClient.reinstateMerchant("G...", "merchant_001");
}
```

## License

MIT
