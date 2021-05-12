#![cfg(feature = "runtime-benchmarks")]

use crate::{BalanceOf, Call, Config, Pallet};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelist_account};
use frame_support::traits::{Currency, Get, ReservableCurrency}; // OnInitialize, OnFinalize
use frame_system::RawOrigin;

/// Default balance amount is minimum contribution
fn default_balance<T: Config>() -> BalanceOf<T> {
    <<T as Config>::MinCollatorStk as Get<BalanceOf<T>>>::get()
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
    T::Currency::make_free_balance_be(&user, total);
    T::Currency::issue(total);
    user
}

/// Create a Contributor.
fn create_contributor<T: Config>(
    string: &'static str,
    relayChainAccount: [u32; 32],
    n: u32,
    contribution: u32,
    reward_ratio: u32,
    extra: BalanceOf<T>,
) -> Result<T::AccountId, &'static str> {
    const SEED: u32 = 0;
    let user = create_funded_user::<T>(string, n, extra);
    Pallet::<T>::initialize_reward_vec(
        RawOrigin::Root,
        vec![relayChainAccount.into(), Some(user.clone()), contribution],
        reward_ratio,
        0,
        1
    )?;
    Ok(user)
}

const USER_SEED: u32 = 999666;

benchmarks! {
    initialize_vec_reward {
        let caller: T::AccountId = create_funded_user::<T>("caller", USER_SEED, 0u32.into());
        let contributions =  vec![relayChainAccount.into(), Some(user), contribution];
        let reward_ratio = 1;
		whitelist_account!(caller);
    }:  _(RawOrigin::Root, contributions, reward_ratio, 0, 1)
	verify {
		assert!(Pallet::<T>::accounts_payable(&caller));
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
    fn bench_join_candidates() {
        new_test_ext().execute_with(|| {
            assert_ok!(test_benchmark_initialize_vec_reward::<Test>());
        });
    }
}

impl_benchmark_test_suite!(
	Pallet,
	crate::benchmarks::tests::new_test_ext(),
	crate::mock::Test
);