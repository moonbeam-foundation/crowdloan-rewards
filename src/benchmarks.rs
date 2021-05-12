#![cfg(feature = "runtime-benchmarks")]

use crate::{BalanceOf, Call, Config, Pallet};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelist_account};
use sp_runtime::traits::One;
use frame_support::traits::{Currency, Get}; // OnInitialize, OnFinalize
use frame_system::RawOrigin;
use frame_support::traits::OnFinalize;
use sp_core::crypto::AccountId32;
/// Default balance amount is minimum contribution
fn default_balance<T: Config>() -> BalanceOf<T> {
    <<T as Config>::MinimumContribution as Get<BalanceOf<T>>>::get()
}

/// Create a funded user.
fn create_funded_user<T: Config>(
    string: &'static str,
    n: u32,
    extra: BalanceOf<T>,
) -> T::AccountId {
    const SEED: u32 = 0;
    let user = account(string, n, SEED);
    let default_balance = default_balance::<T>();
    let total = default_balance + extra;
    T::RewardCurrency::make_free_balance_be(&user, total);
    T::RewardCurrency::issue(total);
    user
}

/// Create a Contributor.
fn create_contributors<T: Config>(
    contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, u32)>,
    reward_ratio: u32,
) -> Result<(), &'static str> {
    Pallet::<T>::initialize_reward_vec(
        RawOrigin::Root.into(),
        contributors.clone(),
        reward_ratio,
        0,
        contributors.len() as u32
    )?;
    Ok(())
}

const USER_SEED: u32 = 999666;
const MAX_USERS: u32 = 10;

benchmarks! {
    initialize_reward_vec {
        let caller: T::AccountId = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let relay_chain_account: AccountId32 = [2u8; 32].into();
        let user: T::AccountId = create_funded_user::<T>("caller", USER_SEED-1, 0u32.into());
        let contribution = 100;
        let contributions =  vec![(relay_chain_account.into(), Some(user.clone()), contribution)];
        let reward_ratio = 1;
		whitelist_account!(caller);
    }:  _(RawOrigin::Root, contributions, reward_ratio, 0, 1)
	verify {
		assert!(Pallet::<T>::accounts_payable(&user).is_some());
	}

	show_me_the_money {
	    let mut contribution_vec = Vec::new();
	    for i in 0..MAX_USERS{
			let seed = MAX_USERS - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
			    account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
            let user = create_funded_user::<T>("user", seed, 0u32.into());
            contribution_vec.push((relay_chain_account.into(), Some(user.clone()), 100));
			whitelist_account!(user);
		}
        create_contributors::<T>(contribution_vec, 1)?;
        let caller: T::AccountId = create_funded_user::<T>("user", MAX_USERS, 0u32.into());

    }: _(RawOrigin::Signed(caller.clone()))
	verify {
	    assert_eq!(Pallet::<T>::accounts_payable(&caller).unwrap().last_paid, T::BlockNumber::one());
	}

	on_finalize_pay_contributors {
	    let mut contribution_vec = Vec::new();


	    for i in 0..MAX_USERS{
			let seed = MAX_USERS - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
			    account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
            let user = create_funded_user::<T>("user", seed, 0u32.into());
            contribution_vec.push((relay_chain_account.into(), Some(user.clone()), 100));
			whitelist_account!(user);
		}

        create_contributors::<T>(contribution_vec, 1)?;
        let caller: T::AccountId = create_funded_user::<T>("user", MAX_USERS, 0u32.into());
    }: {
	    Pallet::<T>::on_finalize(T::BlockNumber::one());
	}
	verify {
	  assert_eq!(Pallet::<T>::accounts_payable(&caller).unwrap().last_paid, T::BlockNumber::one());
	}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::Test;
    use frame_support::assert_ok;
    use sp_io::TestExternalities;

    pub fn new_test_ext() -> TestExternalities {
        let t = frame_system::GenesisConfig::default()
            .build_storage::<Test>()
            .unwrap();
        TestExternalities::new(t)
    }

    #[test]
    fn bench_init_reward_vec() {
        new_test_ext().execute_with(|| {
            assert_ok!(test_benchmark_initialize_reward_vec::<Test>());
        });
    }
    #[test]
    fn bench_show_me_the_money() {
        new_test_ext().execute_with(|| {
            assert_ok!(test_benchmark_show_me_the_money::<Test>());
        });
    }
}

impl_benchmark_test_suite!(
	Pallet,
	crate::benchmarks::tests::new_test_ext(),
	crate::mock::Test
);