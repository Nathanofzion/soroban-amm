use crate::admin::get_admin;
use crate::pool_contract::StandardLiquidityPoolClient;
use crate::storage;
use crate::storage::{
    get_constant_product_pool_hash, get_reward_token, get_stableswap_pool_hash, get_token_hash,
    LiquidityPoolType,
};
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec};

pub fn get_standard_pool_salt(e: &Env, fee_fraction: &u32) -> BytesN<32> {
    let mut salt = Bytes::new(e);
    salt.append(&symbol_short!("standard").to_xdr(&e));
    salt.append(&symbol_short!("0x00").to_xdr(&e));
    salt.append(&fee_fraction.to_xdr(&e));
    salt.append(&symbol_short!("0x00").to_xdr(&e));
    e.crypto().sha256(&salt)
}

pub fn get_stableswap_pool_salt(e: &Env) -> BytesN<32> {
    let mut salt = Bytes::new(e);
    salt.append(&symbol_short!("0x00").to_xdr(e));
    salt.append(&storage::get_stable_swap_next_counter(e).to_xdr(&e));
    salt.append(&symbol_short!("0x00").to_xdr(e));
    e.crypto().sha256(&salt)
}

pub fn get_custom_salt(e: &Env, pool_type: &Symbol, init_args: &Vec<Val>) -> BytesN<32> {
    let mut salt = Bytes::new(e);
    salt.append(&pool_type.to_xdr(e));
    salt.append(&symbol_short!("0x00").to_xdr(e));
    for arg in init_args.clone().into_iter() {
        salt.append(&arg.to_xdr(e));
        salt.append(&symbol_short!("0x00").to_xdr(e));
    }
    e.crypto().sha256(&salt)
}

pub fn merge_salt(e: &Env, left: BytesN<32>, right: BytesN<32>) -> BytesN<32> {
    let mut salt = Bytes::new(e);
    salt.append(&left.to_xdr(e));
    salt.append(&right.to_xdr(e));
    e.crypto().sha256(&salt)
}

pub fn deploy_standard_pool(
    e: &Env,
    tokens: Vec<Address>,
    fee_fraction: u32,
) -> (BytesN<32>, Address) {
    let salt = crate::utils::pool_salt(e, tokens.clone());
    let liquidity_pool_wasm_hash = get_constant_product_pool_hash(&e);
    let subpool_salt = get_standard_pool_salt(e, &fee_fraction);

    let pool_contract_id = e
        .deployer()
        .with_current_contract(merge_salt(e, salt.clone(), subpool_salt.clone()))
        .deploy(liquidity_pool_wasm_hash);
    init_standard_pool(e, &tokens, &pool_contract_id, fee_fraction);

    storage::add_pool(
        e,
        &salt,
        subpool_salt.clone(),
        LiquidityPoolType::ConstantProduct as u32,
        pool_contract_id.clone(),
    );

    e.events().publish(
        (Symbol::new(e, "add_pool"), tokens.clone()),
        (
            &pool_contract_id,
            symbol_short!("constant"),
            subpool_salt.clone(),
            Vec::<Val>::from_array(e, [fee_fraction.into_val(e)]),
        ),
    );

    (subpool_salt, pool_contract_id)
}

pub fn deploy_stableswap_pool(
    e: &Env,
    tokens: Vec<Address>,
    a: u128,
    fee_fraction: u32,
    admin_fee: u32,
) -> (BytesN<32>, Address) {
    let salt = crate::utils::pool_salt(&e, tokens.clone());

    let liquidity_pool_wasm_hash = get_stableswap_pool_hash(&e, tokens.len());
    let subpool_salt = get_stableswap_pool_salt(&e);

    let pool_contract_id = e
        .deployer()
        .with_current_contract(merge_salt(&e, salt.clone(), subpool_salt.clone()))
        .deploy(liquidity_pool_wasm_hash);
    init_stableswap_pool(e, &tokens, &pool_contract_id, a, fee_fraction, admin_fee);

    // if STABLE_SWAP_MAX_POOLS
    storage::add_pool(
        &e,
        &salt,
        subpool_salt.clone(),
        LiquidityPoolType::StableSwap as u32,
        pool_contract_id.clone(),
    );

    e.events().publish(
        (Symbol::new(&e, "add_pool"), tokens.clone()),
        (
            &pool_contract_id,
            symbol_short!("stable"),
            subpool_salt.clone(),
            Vec::<Val>::from_array(
                e,
                [
                    fee_fraction.into_val(e),
                    a.into_val(e),
                    admin_fee.into_val(e),
                ],
            ),
        ),
    );

    (subpool_salt, pool_contract_id)
}

fn init_standard_pool(
    e: &Env,
    tokens: &Vec<Address>,
    pool_contract_id: &Address,
    fee_fraction: u32,
) {
    let token_wasm_hash = get_token_hash(&e);
    let reward_token = get_reward_token(&e);
    let admin = get_admin(&e);
    let liq_pool_client = StandardLiquidityPoolClient::new(&e, pool_contract_id);
    liq_pool_client.initialize(&admin, &token_wasm_hash, tokens, &fee_fraction);
    liq_pool_client.initialize_rewards_config(&reward_token, &e.current_contract_address());
}

fn init_stableswap_pool(
    e: &Env,
    tokens: &Vec<Address>,
    pool_contract_id: &Address,
    a: u128,
    fee_fraction: u32,
    admin_fee_fraction: u32,
) {
    let token_wasm_hash = get_token_hash(&e);
    let reward_token = get_reward_token(&e);
    let admin = get_admin(&e);
    e.invoke_contract::<bool>(
        pool_contract_id,
        &Symbol::new(&e, "initialize"),
        Vec::from_array(
            &e,
            [
                admin.into_val(e),
                token_wasm_hash.into_val(e),
                tokens.clone().into_val(e),
                a.into_val(e),
                (fee_fraction as u128).into_val(e),
                (admin_fee_fraction as u128).into_val(e),
            ],
        ),
    );
    e.invoke_contract::<bool>(
        pool_contract_id,
        &Symbol::new(&e, "initialize_rewards_config"),
        Vec::from_array(
            &e,
            [
                reward_token.into_val(e),
                e.current_contract_address().into_val(e),
            ],
        ),
    );
}
