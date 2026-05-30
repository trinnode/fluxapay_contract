use crate::{
    merchant_registry::{KycTier, MerchantRegistry, MerchantRegistryClient},
    DisputeStatus, PaymentProcessor, PaymentProcessorClient, PaymentStatus, RefundManager,
    RefundManagerClient, RefundStatus, SettlementSplit,
};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Ledger as _},
    token, vec, Address, BytesN, Env, String, Symbol,
};

fn setup_integration(
    env: &Env,
) -> (
    Address,
    PaymentProcessorClient<'_>,
    RefundManagerClient<'_>,
    MerchantRegistryClient<'_>,
) {
    let payment_processor = env.register(PaymentProcessor, ());
    let refund_manager = env.register(RefundManager, ());
    let merchant_registry = env.register(MerchantRegistry, ());

    let refund_client = RefundManagerClient::new(env, &refund_manager);
    let payment_client = PaymentProcessorClient::new(env, &payment_processor);
    let merchant_client = MerchantRegistryClient::new(env, &merchant_registry);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    refund_client.initialize_refund_manager(&admin, &usdc_token);
    let token_admin_client = token::StellarAssetClient::new(env, &usdc_token);
    token_admin_client.mint(&refund_manager, &1_000_000_000_000i128);

    payment_client.initialize_payment_processor(&admin);
    merchant_client.initialize(&admin);

    (admin, payment_client, refund_client, merchant_client)
}

#[test]
fn test_happy_path_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, refund_client, merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // 1. Register and Verify Merchant
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Flux Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);
    let merchant_info = merchant_client.get_merchant(&merchant);
    assert_eq!(merchant_info.kyc_tier, KycTier::Basic);

    // 2. Create and Verify Payment
    let payment_id = String::from_str(&env, "PAY_01");
    let amount = 1000i128;

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let tx_hash = BytesN::<32>::random(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &tx_hash, &customer, &amount);

    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.status, PaymentStatus::Confirmed);

    // Register payment with refund manager for amount validation
    refund_client.register_payment(&payment_id, &merchant, &amount, &Symbol::new(&env, "USDC"));

    // 3. Create Dispute and Resolve with Refund
    let dispute_id = refund_client.create_dispute(
        &payment_id,
        &amount,
        &String::from_str(&env, "Product Damaged"),
        &String::from_str(&env, "Video evidence"),
        &customer,
        &vec![&env],
    );

    let operator = Address::generate(&env);
    refund_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    let refund_id = refund_client.resolve_dispute_with_refund(
        &operator,
        &dispute_id,
        &String::from_str(&env, "Refund approved"),
        &String::from_str(&env, "base64sig=="),
    );

    let dispute_info = refund_client.get_dispute(&dispute_id);
    assert_eq!(dispute_info.status, DisputeStatus::Resolved);
    assert!(dispute_info.refund_id.is_some());

    let refund_info = refund_client.get_refund(&refund_id);
    assert_eq!(refund_info.status, RefundStatus::Completed);
}

#[test]
fn test_settlement_path() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let treasury = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    let payment_id = String::from_str(&env, "PAY_SETTLE");
    let amount = 2000i128;
    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(
        &oracle,
        &payment_id,
        &BytesN::<32>::random(&env),
        &customer,
        &amount,
    );

    // Settle payment to treasury as a single split
    let splits = vec![
        &env,
        SettlementSplit {
            recipient: treasury.clone(),
            amount,
        },
    ];
    payment_client.settle_payment(&operator, &payment_id, &splits);

    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.status, PaymentStatus::Settled);
}

#[test]
fn test_failure_and_expiration_path() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);

    let payment_id = String::from_str(&env, "PAY_EXPIRE");
    let amount = 500i128;
    let expires_at = env.ledger().timestamp() + 100;

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(expires_at),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    // Jump forward in time
    env.ledger().set_timestamp(expires_at + 1);

    // Expire payment via cleanup path
    payment_client.expire_payment(&payment_id);

    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.status, PaymentStatus::Expired);

    // Register payment with refund manager (with Confirmed status for testing)
    refund_client.register_payment(&payment_id, &merchant, &amount, &Symbol::new(&env, "USDC"));

    // Try to dispute an expired/cancelled payment - should still be possible to create, but maybe rejected?
    let customer = Address::generate(&env);
    let dispute_id = refund_client.create_dispute(
        &payment_id,
        &amount,
        &String::from_str(&env, "Late but flawed"),
        &String::from_str(&env, "N/A"),
        &customer,
        &vec![&env],
    );

    let operator = Address::generate(&env);
    refund_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &operator);

    // Reject dispute
    refund_client.reject_dispute(
        &operator,
        &dispute_id,
        &String::from_str(&env, "Payment already expired and cancelled"),
        &String::from_str(&env, "base64sig=="),
    );

    let dispute_info = refund_client.get_dispute(&dispute_id);
    assert_eq!(dispute_info.status, DisputeStatus::Rejected);
}

#[test]
fn test_upgrade_contract_payment_processor() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);

    let new_wasm_hash = BytesN::<32>::random(&env);

    // Admin can upgrade
    let result = payment_client.upgrade_contract(&admin, &new_wasm_hash);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_contract_payment_processor_non_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let non_admin = Address::generate(&env);

    let new_wasm_hash = BytesN::<32>::random(&env);

    // Non-admin cannot upgrade
    let result = payment_client.upgrade_contract(&non_admin, &new_wasm_hash);
    assert!(result.is_err());
}

#[test]
fn test_upgrade_contract_refund_manager() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _payment_client, refund_client, _merchant_client) = setup_integration(&env);

    let new_wasm_hash = BytesN::<32>::random(&env);

    // Admin can upgrade
    let result = refund_client.upgrade_contract(&admin, &new_wasm_hash);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_contract_refund_manager_non_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, _payment_client, refund_client, _merchant_client) = setup_integration(&env);
    let non_admin = Address::generate(&env);

    let new_wasm_hash = BytesN::<32>::random(&env);

    // Non-admin cannot upgrade
    let result = refund_client.upgrade_contract(&non_admin, &new_wasm_hash);
    assert!(result.is_err());
}

#[test]
fn test_upgrade_contract_storage_compatibility() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);

    // Create a payment before upgrade to test storage compatibility
    let payment_id = String::from_str(&env, "PAY_UPGRADE");
    let amount = 1000i128;

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    // Verify payment exists before upgrade
    let payment_before = payment_client.get_payment(&payment_id);
    assert_eq!(payment_before.amount, amount);

    // Perform upgrade
    let new_wasm_hash = BytesN::<32>::random(&env);
    let result = payment_client.upgrade_contract(&admin, &new_wasm_hash);
    assert!(result.is_ok());

    // Verify payment still exists after upgrade (storage compatibility)
    let payment_after = payment_client.get_payment(&payment_id);
    assert_eq!(payment_after.amount, amount);
}

#[test]
fn test_prune_expired_payments_expired_pending() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Create an expired pending payment
    let payment_id = String::from_str(&env, "PAY_EXPIRE_PRUNE");
    let amount = 1000i128;
    let expires_at = env.ledger().timestamp() + 100;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(expires_at),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    // Jump forward in time to expire the payment
    env.ledger().set_timestamp(expires_at + 1);

    // Prune the expired payment
    let payment_ids = vec![&env, payment_id.clone()];
    let result = payment_client.prune_expired_payments(&operator, &payment_ids);
    assert_eq!(result.unwrap(), 1);

    // Verify payment is deleted
    let get_result = payment_client.get_payment(&payment_id);
    assert!(get_result.is_err());
}

#[test]
fn test_prune_expired_payments_non_expired_skipped() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Create a non-expired pending payment
    let payment_id = String::from_str(&env, "PAY_NOT_EXPIRE");
    let amount = 1000i128;
    let expires_at = env.ledger().timestamp() + 3600;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(expires_at),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    // Prune (payment should not be pruned as it's not expired)
    let payment_ids = vec![&env, payment_id.clone()];
    let result = payment_client.prune_expired_payments(&operator, &payment_ids);
    assert_eq!(result.unwrap(), 0);

    // Verify payment still exists
    let payment_info = payment_client.get_payment(&payment_id);
    assert!(payment_info.is_ok());
}

#[test]
fn test_prune_expired_payments_non_pending_skipped() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let oracle = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Create a confirmed (non-pending) payment
    let payment_id = String::from_str(&env, "PAY_CONFIRMED");
    let amount = 1000i128;
    let expires_at = env.ledger().timestamp() + 100;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(expires_at),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    // Verify the payment to change it from Pending to Confirmed
    let customer = Address::generate(&env);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Jump forward in time
    env.ledger().set_timestamp(expires_at + 1);

    // Prune (confirmed payment should not be pruned)
    let payment_ids = vec![&env, payment_id.clone()];
    let result = payment_client.prune_expired_payments(&operator, &payment_ids);
    assert_eq!(result.unwrap(), 0);

    // Verify payment still exists
    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.unwrap().status, PaymentStatus::Confirmed);
}

#[test]
fn test_prune_expired_payments_unauthorized_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let non_operator = Address::generate(&env);

    // Try to prune as non-operator
    let payment_ids = vec![&env];
    let result = payment_client.prune_expired_payments(&non_operator, &payment_ids);
    assert!(result.is_err());
}

#[test]
fn test_prune_expired_payments_empty_list() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Prune with empty list
    let payment_ids = vec![&env];
    let result = payment_client.prune_expired_payments(&operator, &payment_ids);
    assert_eq!(result.unwrap(), 0);
}

#[test]
fn test_settle_payment_with_zero_merchant_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Register merchant with zero fee
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Test Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);
    merchant_client.set_fee_config(&admin, &merchant, &0, &0, &None::<Address>);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    // Create and settle payment
    let payment_id = String::from_str(&env, "PAY_ZERO_FEE");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let customer = Address::generate(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Settlement should work with registry and fee config
    let splits = vec![&env, crate::SettlementSplit {
        recipient: merchant.clone(),
        amount,
    }];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let payment = payment_client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.status, PaymentStatus::Settled);
}

#[test]
fn test_settle_payment_with_bps_only_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Register merchant with 5% BPS fee
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Test Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);
    merchant_client.set_fee_config(&admin, &merchant, &500, &0, &None::<Address>);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    // Create and settle payment
    let payment_id = String::from_str(&env, "PAY_BPS_FEE");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let customer = Address::generate(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Settlement should work with registry and fee config
    let splits = vec![&env, crate::SettlementSplit {
        recipient: merchant.clone(),
        amount,
    }];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let payment = payment_client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.status, PaymentStatus::Settled);
}

#[test]
fn test_settle_payment_with_fixed_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Register merchant with fixed fee of 100
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Test Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);
    merchant_client.set_fee_config(&admin, &merchant, &0, &100, &None::<Address>);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    // Create and settle payment
    let payment_id = String::from_str(&env, "PAY_FIXED_FEE");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let customer = Address::generate(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Settlement should work with registry and fee config
    let splits = vec![&env, crate::SettlementSplit {
        recipient: merchant.clone(),
        amount,
    }];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let payment = payment_client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.status, PaymentStatus::Settled);
}

#[test]
fn test_settle_payment_with_combined_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Register merchant with 2% BPS + 50 fixed fee
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Test Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);
    merchant_client.set_fee_config(&admin, &merchant, &200, &50, &None::<Address>);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    // Create and settle payment
    let payment_id = String::from_str(&env, "PAY_COMBINED_FEE");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let customer = Address::generate(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Settlement should work with registry and fee config
    let splits = vec![&env, crate::SettlementSplit {
        recipient: merchant.clone(),
        amount,
    }];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let payment = payment_client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.status, PaymentStatus::Settled);
}

#[test]
fn test_settle_payment_no_registry_configured() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, _merchant_client) = setup_integration(&env);
    let merchant = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // No registry configured - existing behavior should work

    let payment_id = String::from_str(&env, "PAY_NO_REGISTRY");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    payment_client.create_payment(&args);

    let customer = Address::generate(&env);
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    payment_client.verify_payment(&oracle, &payment_id, &BytesN::<32>::random(&env), &customer, &amount);

    // Settlement with splits should work without registry
    let splits = vec![&env, crate::SettlementSplit {
        recipient: Address::generate(&env),
        amount,
    }];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let payment = payment_client.get_payment(&payment_id).unwrap();
    assert_eq!(payment.status, PaymentStatus::Settled);
}

#[test]
fn test_cross_contract_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let operator = Address::generate(&env);

    payment_client.grant_role(&admin, &Symbol::new(&env, "SETTLEMENT_OPERATOR"), &operator);

    // Happy path: register merchant → verify merchant → create_payment → confirm payment → settle
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Flux Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );

    let merchant_info = merchant_client.get_merchant(&merchant);
    assert_eq!(merchant_info.kyc_tier, KycTier::Unverified);

    merchant_client.verify_merchant(&admin, &merchant);
    let verified_merchant = merchant_client.get_merchant(&merchant);
    assert_eq!(verified_merchant.kyc_tier, KycTier::Basic);

    // Now create payment with verified merchant
    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let payment_id = String::from_str(&env, "PAY_CROSS_01");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };
    let result = payment_client.create_payment(&args);
    assert!(result.is_ok());

    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.unwrap().status, PaymentStatus::Pending);

    // Confirm payment
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    let tx_hash = BytesN::<32>::random(&env);
    let result = payment_client.verify_payment(&oracle, &payment_id, &tx_hash, &customer, &amount);
    assert!(result.is_ok());

    let confirmed = payment_client.get_payment(&payment_id);
    assert_eq!(confirmed.unwrap().status, PaymentStatus::Confirmed);

    // Settle payment
    let splits = vec![
        &env,
        SettlementSplit {
            recipient: merchant.clone(),
            amount,
        },
    ];
    let result = payment_client.settle_payment(&operator, &payment_id, &splits);
    assert!(result.is_ok());

    let settled = payment_client.get_payment(&payment_id);
    assert_eq!(settled.unwrap().status, PaymentStatus::Settled);
}

#[test]
fn test_cross_contract_unverified_merchant_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    let merchant = Address::generate(&env);

    // Register but don't verify merchant
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Flux Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );

    // Try to create payment with unverified merchant
    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let payment_id = String::from_str(&env, "PAY_UNVERIFIED");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };

    // Creating payment with unverified merchant should fail
    let result = payment_client.create_payment(&args);
    assert!(result.is_err());
}

#[test]
fn test_cross_contract_suspended_merchant_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);

    // Wire payment processor to merchant registry
    payment_client.set_merchant_registry_address(&admin, &merchant_client.address());

    let merchant = Address::generate(&env);

    // Register and verify merchant
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Flux Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );
    merchant_client.verify_merchant(&admin, &merchant);

    // Suspend the merchant
    merchant_client.suspend_merchant(
        &admin,
        &merchant,
        &String::from_str(&env, "Suspicious activity"),
        &None::<u64>,
    );

    // Try to create payment with suspended merchant
    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let payment_id = String::from_str(&env, "PAY_SUSPENDED");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };

    // Creating payment with suspended merchant should fail
    let result = payment_client.create_payment(&args);
    assert!(result.is_err());
}

#[test]
fn test_cross_contract_registry_not_set_regression() {
    let env = Env::default();
    env.mock_all_auths();

    let (admin, payment_client, _refund_client, merchant_client) = setup_integration(&env);

    // Don't wire payment processor to merchant registry - regression test

    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);

    // Register merchant in merchant registry only
    merchant_client.register_merchant(
        &merchant,
        &String::from_str(&env, "Flux Merchant"),
        &String::from_str(&env, "USD"),
        &None::<Address>,
        &None::<String>,
        &None,
    );

    // Try to create payment without registry wired
    // Should check merchant role, not merchant verification status
    payment_client.grant_role(&admin, &Symbol::new(&env, "MERCHANT"), &merchant);
    let payment_id = String::from_str(&env, "PAY_NO_REG");
    let amount = 1000i128;

    let args = crate::CreatePaymentArgs {
        payment_id: payment_id.clone(),
        merchant_id: merchant.clone(),
        amount,
        currency: Symbol::new(&env, "USDC"),
        deposit_address: Address::generate(&env),
        expires_at: Some(env.ledger().timestamp() + 3600),
        duration_secs: None,
        memo: None,
        memo_type: None,
        token_address: None,
        client_token: None,
        metadata_hash: None,
    };

    // Should succeed because merchant has MERCHANT role (registry check skipped)
    let result = payment_client.create_payment(&args);
    assert!(result.is_ok());

    // Verify payment
    let oracle = Address::generate(&env);
    payment_client.grant_role(&admin, &Symbol::new(&env, "ORACLE"), &oracle);
    let tx_hash = BytesN::<32>::random(&env);
    let result = payment_client.verify_payment(&oracle, &payment_id, &tx_hash, &customer, &amount);
    assert!(result.is_ok());

    let payment_info = payment_client.get_payment(&payment_id);
    assert_eq!(payment_info.unwrap().status, PaymentStatus::Confirmed);
}
