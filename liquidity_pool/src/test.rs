#![cfg(test)]
extern crate std;

use crate::{token, LiquidityPoolClient};

use soroban_sdk::testutils::{AuthorizedFunction, AuthorizedInvocation, Ledger, LedgerInfo};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, IntoVal, Symbol};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> token::Client<'a> {
    token::Client::new(e, &e.register_stellar_asset_contract(admin.clone()))
}

fn create_liqpool_contract<'a>(
    e: &Env,
    admin: &Address,
    token_wasm_hash: &BytesN<32>,
    token_a: &Address,
    token_b: &Address,
    token_reward: &Address,
) -> LiquidityPoolClient<'a> {
    let liqpool = LiquidityPoolClient::new(e, &e.register_contract(None, crate::LiquidityPool {}));
    liqpool.initialize(admin, token_wasm_hash, token_a, token_b, token_reward, &liqpool.address);
    liqpool
}

fn install_token_wasm(e: &Env) -> BytesN<32> {
    soroban_sdk::contractimport!(
        file = "../token/target/wasm32-unknown-unknown/release/soroban_token_contract.wasm"
    );
    e.deployer().upload_contract_wasm(WASM)
}

fn jump(e: &Env, time: u64) {
    e.ledger().set(LedgerInfo {
        timestamp: e.ledger().timestamp().saturating_add(time),
        protocol_version: 20,
        sequence_number: e.ledger().sequence(),
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_expiration: 999999,
        min_persistent_entry_expiration: 999999,
        max_entry_expiration: u32::MAX,
    });
}

#[test]
fn test() {
    let e = Env::default();
    e.mock_all_auths();
    e.budget().reset_unlimited();

    let mut admin1 = Address::random(&e);
    let mut admin2 = Address::random(&e);

    let mut token1 = create_token_contract(&e, &admin1);
    let mut token2 = create_token_contract(&e, &admin2);
    let token_reward = create_token_contract(&e, &admin1);
    if &token2.address < &token1.address {
        std::mem::swap(&mut token1, &mut token2);
        std::mem::swap(&mut admin1, &mut admin2);
    }
    let user1 = Address::random(&e);
    let liqpool = create_liqpool_contract(
        &e,
        &user1,
        &install_token_wasm(&e),
        &token1.address,
        &token2.address,
        &token_reward.address,
    );

    token_reward.mint(&liqpool.address, &1_000_000_0000000);
    let total_reward_1 = 10_5000000_i128 * 60;
    liqpool.set_rewards_config(&user1, &e.ledger().timestamp().saturating_add(60), &total_reward_1);
    token_reward.approve(&liqpool.address, &liqpool.address, &1_000_000_0000000, &99999);

    let token_share = token::Client::new(&e, &liqpool.share_id());

    token1.mint(&user1, &1000);
    assert_eq!(token1.balance(&user1), 1000);

    token2.mint(&user1, &1000);
    assert_eq!(token2.balance(&user1), 1000);
    token1.approve(&user1, &liqpool.address, &1000, &99999);
    token2.approve(&user1, &liqpool.address, &1000, &99999);

    liqpool.deposit(&user1, &100, &100, &100, &100);
    assert_eq!(
        e.auths()[0],
        (
            user1.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    liqpool.address.clone(),
                    Symbol::new(&e, "deposit"),
                    (&user1, 100_i128, 100_i128, 100_i128, 100_i128).into_val(&e)
                )),
                sub_invocations: std::vec![],
            }
        )
    );

    assert_eq!(token_reward.balance(&user1), 0);
    // 30 seconds passed, half of the reward is available for the user
    jump(&e, 30);
    assert_eq!(liqpool.claim(&user1), total_reward_1 / 2);
    assert_eq!(token_reward.balance(&user1), total_reward_1 / 2);
    // 60 seconds more passed. full reward was available though half already claimed
    jump(&e, 60);
    assert_eq!(liqpool.claim(&user1), total_reward_1 / 2);
    assert_eq!(token_reward.balance(&user1), total_reward_1);

    // more rewards added with different configs
    let total_reward_2 = 20_0000000_i128 * 100;
    liqpool.set_rewards_config(&user1, &e.ledger().timestamp().saturating_add(100), &total_reward_2);
    jump(&e, 105);
    let total_reward_3 = 6_0000000_i128 * 50;
    liqpool.set_rewards_config(&user1, &e.ledger().timestamp().saturating_add(50), &total_reward_3);
    jump(&e, 500);
    // two rewards available for the user
    assert_eq!(liqpool.claim(&user1), total_reward_2 + total_reward_3);
    assert_eq!(
        token_reward.balance(&user1),
        total_reward_1 + total_reward_2 + total_reward_3
    );

    assert_eq!(token_share.balance(&user1), 100);
    assert_eq!(token_share.balance(&liqpool.address), 0);
    assert_eq!(token1.balance(&user1), 900);
    assert_eq!(token1.balance(&liqpool.address), 100);
    assert_eq!(token2.balance(&user1), 900);
    assert_eq!(token2.balance(&liqpool.address), 100);

    assert_eq!(liqpool.estimate_swap_out(&false, &49), 97_i128,);
    liqpool.swap(&user1, &false, &49, &100);
    assert_eq!(
        e.auths()[0],
        (
            user1.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    liqpool.address.clone(),
                    Symbol::new(&e, "swap"),
                    (&user1, false, 49_i128, 100_i128).into_val(&e)
                )),
                sub_invocations: std::vec![],
            }
        )
    );

    assert_eq!(token1.balance(&user1), 803);
    assert_eq!(token1.balance(&liqpool.address), 197);
    assert_eq!(token2.balance(&user1), 949);
    assert_eq!(token2.balance(&liqpool.address), 51);

    token_share.approve(&user1, &liqpool.address, &100, &99999);

    liqpool.withdraw(&user1, &100, &197, &51);
    assert_eq!(
        e.auths()[0],
        (
            user1.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    liqpool.address.clone(),
                    Symbol::new(&e, "withdraw"),
                    (&user1, 100_i128, 197_i128, 51_i128).into_val(&e)
                )),
                sub_invocations: std::vec![],
            }
        )
    );

    jump(&e, 600);
    assert_eq!(liqpool.claim(&user1), 0);
    assert_eq!(
        token_reward.balance(&user1),
        total_reward_1 + total_reward_2 + total_reward_3
    );

    assert_eq!(token1.balance(&user1), 1000);
    assert_eq!(token2.balance(&user1), 1000);
    assert_eq!(token_share.balance(&user1), 0);
    assert_eq!(token1.balance(&liqpool.address), 0);
    assert_eq!(token2.balance(&liqpool.address), 0);
    assert_eq!(token_share.balance(&liqpool.address), 0);
}

#[test]
fn test_two_users_rewards() {
    let e = Env::default();
    e.mock_all_auths();
    e.budget().reset_unlimited();

    let mut admin1 = Address::random(&e);
    let mut admin2 = Address::random(&e);

    let mut token1 = create_token_contract(&e, &admin1);
    let mut token2 = create_token_contract(&e, &admin2);
    let token_reward = create_token_contract(&e, &admin1);
    if &token2.address < &token1.address {
        std::mem::swap(&mut token1, &mut token2);
        std::mem::swap(&mut admin1, &mut admin2);
    }
    let user1 = Address::random(&e);
    let user2 = Address::random(&e);

    let liqpool = create_liqpool_contract(
        &e,
        &user1,
        &install_token_wasm(&e),
        &token1.address,
        &token2.address,
        &token_reward.address,
    );

    token_reward.mint(&liqpool.address, &1_000_000_0000000);
    let total_reward_1 = 10_5000000_i128 * 60;
    liqpool.set_rewards_config(&user1, &e.ledger().timestamp().saturating_add(60), &total_reward_1);
    token_reward.approve(&liqpool.address, &liqpool.address, &1_000_000_0000000, &99999);

    for user in [&user1, &user2] {
        token1.mint(user, &1000);
        assert_eq!(token1.balance(user), 1000);

        token2.mint(user, &1000);
        assert_eq!(token2.balance(user), 1000);

        token1.approve(user, &liqpool.address, &1000, &99999);
        token2.approve(user, &liqpool.address, &1000, &99999);
    }

    // two users make deposit for equal value. second after 30 seconds after rewards start,
    //  so it gets only 1/4 of total reward
    liqpool.deposit(&user1, &100, &100, &100, &100);
    jump(&e, 30);
    assert_eq!(liqpool.claim(&user1), total_reward_1 / 2);
    liqpool.deposit(&user2, &100, &100, &100, &100);
    jump(&e, 100);
    assert_eq!(liqpool.claim(&user1), total_reward_1 / 4);
    assert_eq!(liqpool.claim(&user2), total_reward_1 / 4);
    assert_eq!(token_reward.balance(&user1), total_reward_1 / 4 * 3);
    assert_eq!(token_reward.balance(&user2), total_reward_1 / 4);
}
