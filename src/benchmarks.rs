#![cfg(feature = "runtime-benchmarks")]

use crate::{BalanceOf, Call, Config, Pallet};
use ed25519_dalek::Signer;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	inherent::{InherentData, ProvideInherent},
	traits::{Currency, Get, OnFinalize, OnInitialize},
};
use frame_system::RawOrigin;
use parity_scale_codec::Encode;
use sp_core::{
	crypto::{AccountId32, UncheckedFrom},
	ed25519,
};
use sp_runtime::{traits::One, MultiSignature};
use sp_runtime::offchain::storage_lock::BlockNumberProvider;
use sp_std::vec;
use sp_std::vec::Vec;
use sp_trie::StorageProof;
// This is a fake proof that emulates a storage proof inserted as the validation data
// We avoid using the sproof builder here because it generates an issue when compiling without std
// Fake storage proof
const MOCK_PROOF: [u8; 71] = [
	127, 1, 6, 222, 61, 138, 84, 210, 126, 68, 169, 213, 206, 24, 150, 24, 242, 45, 180, 180, 157,
	149, 50, 13, 144, 33, 153, 76, 133, 15, 37, 184, 227, 133, 144, 0, 0, 32, 0, 0, 0, 16, 0, 8, 0,
	0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 5, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 6, 0, 0, 0,
];

// fake storage root. This is valid with the previous proof
const MOCK_PROOF_HASH: [u8; 32] = [
	216, 6, 227, 175, 180, 211, 98, 117, 202, 245, 206, 51, 21, 143, 100, 232, 96, 217, 14, 71,
	243, 146, 7, 202, 245, 129, 165, 70, 72, 184, 130, 141,
];

/// Default balance amount is minimum contribution
fn default_balance<T: Config>() -> BalanceOf<T> {
	<<T as Config>::MinimumReward as Get<BalanceOf<T>>>::get()
}

/// Create a funded user.
fn fund_specific_account<T: Config>(pallet_account: T::AccountId, extra: BalanceOf<T>) {
	let default_balance = default_balance::<T>();
	let total = default_balance + extra;
	T::RewardCurrency::make_free_balance_be(&pallet_account, total);
	T::RewardCurrency::issue(total);
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

/// Create contributors.
fn create_contributors<T: Config>(
	total_number: u32,
	seed_offset: u32,
) -> Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)> {
	let mut contribution_vec = Vec::new();
	for i in 0..total_number {
		let seed = SEED - seed_offset - i;
		let mut account: [u8; 32] = [0u8; 32];
		let seed_as_slice = seed.to_be_bytes();
		for j in 0..seed_as_slice.len() {
			account[j] = seed_as_slice[j]
		}
		let relay_chain_account: AccountId32 = account.into();
		let user = create_funded_user::<T>("user", seed, 0u32.into());
		let contribution: BalanceOf<T> = 100u32.into();
		contribution_vec.push((relay_chain_account.into(), Some(user.clone()), contribution));
	}
	contribution_vec
}

/// Insert contributors.
fn insert_contributors<T: Config>(
	contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)>,
) -> Result<(), &'static str> {
	let mut sub_vec = Vec::new();
	let batch = max_batch_contributors::<T>();
	// Due to the MaxInitContributors associated type, we need ton insert them in batches
	// When we reach the batch size, we insert them
	for i in 0..contributors.len() {
		sub_vec.push(contributors[i].clone());
		// If we reached the batch size, we should insert them
		if i as u32 % batch == batch - 1 || i == contributors.len() - 1 {
			Pallet::<T>::initialize_reward_vec(RawOrigin::Root.into(), sub_vec.clone())?;
			sub_vec.clear()
		}
	}
	Ok(())
}

/// Create a Contributor.
fn close_initialization<T: Config>(end_relay: T::VestingBlockNumber) -> Result<(), &'static str> {
	Pallet::<T>::complete_initialization(RawOrigin::Root.into(), end_relay)?;
	Ok(())
}

fn create_sig<T: Config>(signed_account: T::AccountId) -> (AccountId32, MultiSignature) {
	let seed: [u8; 32] = [1u8; 32];
	let secret = ed25519_dalek::SecretKey::from_bytes(&seed).unwrap();
	let public = ed25519_dalek::PublicKey::from(&secret);
	let pair = ed25519_dalek::Keypair { secret, public };
	let sig = pair.sign(&signed_account.encode()).to_bytes();
	let signature: MultiSignature = ed25519::Signature::from_raw(sig).into();

	let ed_public: ed25519::Public = ed25519::Public::unchecked_from(public.to_bytes());
	let account: AccountId32 = ed_public.into();
	(account, signature.into())
}

fn max_batch_contributors<T: Config>() -> u32 {
	<<T as Config>::MaxInitContributors as Get<u32>>::get()
}

// This is our current number of contributors
const MAX_ALREADY_USERS: u32 = 5799;
const SEED: u32 = 999999999;
benchmarks! {
	initialize_reward_vec {
		let x in 1..max_batch_contributors::<T>();
		let y = MAX_ALREADY_USERS;

		let total_pot = 100u32*(x+y);
		// We probably need to assume we have N contributors already in
		// Fund pallet account
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());

		// Create y contributors
		let contributors = create_contributors::<T>(y, 0);

		// Insert them
		insert_contributors::<T>(contributors)?;

		// This X new contributors are the ones we will count
		let new_contributors = create_contributors::<T>(x, y);

		let verifier = create_funded_user::<T>("user", SEED, 0u32.into());

	}:  _(RawOrigin::Root, new_contributors)
	verify {
		assert!(Pallet::<T>::accounts_payable(&verifier).is_some());
	}

	complete_initialization {
		// Fund pallet account
		let total_pot = 100u32;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());
		// 1 contributor is enough
		let contributors = create_contributors::<T>(1, 0);

		// Insert them
		insert_contributors::<T>(contributors)?;

		// We need to create the first block inherent, to initialize the initRelayBlock

		T::VestingBlockProvider::current_block_number();
		Pallet::<T>::on_finalize(T::BlockNumber::one());

	}:  _(RawOrigin::Root, 10u32.into())
	verify {
	  assert!(Pallet::<T>::initialized());
	}

	claim {
		// Fund pallet account
		let total_pot = 100u32;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());

		// The user that will make the call
		let caller: T::AccountId = create_funded_user::<T>("user", SEED, 100u32.into());

		// We verified there is no dependency of the number of contributors already inserted in claim
		// Create 1 contributor
		let contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)> =
			vec![(AccountId32::from([1u8;32]).into(), Some(caller.clone()), total_pot.into())];

		// Insert them
		insert_contributors::<T>(contributors)?;

		// Close initialization
		close_initialization::<T>(10u32.into())?;

		// First inherent
		T::VestingBlockProvider::current_block_number();
		Pallet::<T>::on_finalize(T::BlockNumber::one());

		// Create 4th relay block, by now the user should have vested some amount
		T::VestingBlockProvider::current_block_number();

	}:  _(RawOrigin::Signed(caller.clone()))
	verify {
	  assert_eq!(Pallet::<T>::accounts_payable(&caller).unwrap().total_reward, (100u32.into()));
	}

	update_reward_address {
		// Fund pallet account
		let total_pot = 100u32;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());

		// The user that will make the call
		let caller: T::AccountId = create_funded_user::<T>("user", SEED, 100u32.into());

		let relay_account: T::RelayChainAccountId = AccountId32::from([1u8;32]).into();
		// We verified there is no dependency of the number of contributors already inserted in update_reward_address
		// Create 1 contributor
		let contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)> =
			vec![(relay_account.clone(), Some(caller.clone()), total_pot.into())];

		// Insert them
		insert_contributors::<T>(contributors)?;

		// Close initialization
		close_initialization::<T>(10u32.into())?;

		// First inherent
		T::VestingBlockProvider::current_block_number();

		Pallet::<T>::on_finalize(T::BlockNumber::one());


		// Let's advance the relay so that the vested  amount get transferred

		T::VestingBlockProvider::current_block_number();

		// The new user
		let new_user = create_funded_user::<T>("user", SEED+1, 0u32.into());

	}:  _(RawOrigin::Signed(caller.clone()), new_user.clone())
	verify {
		assert_eq!(Pallet::<T>::accounts_payable(&new_user).unwrap().total_reward, (100u32.into()));
		assert!(Pallet::<T>::claimed_relay_chain_ids(&relay_account).is_some());
	}

	associate_native_identity {
		// Fund pallet account
		let total_pot = 100u32;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());

		// The caller that will associate the account
		let caller: T::AccountId = create_funded_user::<T>("user", SEED, 100u32.into());

		// Create a fake sig for such an account
		let (relay_account, signature) = create_sig::<T>(caller.clone());

		// We verified there is no dependency of the number of contributors already inserted in associate_native_identity
		// Create 1 contributor
		let contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)> =
		vec![(relay_account.clone().into(), None, total_pot.into())];

		// Insert them
		insert_contributors::<T>(contributors)?;

		// Clonse initialization
		close_initialization::<T>(10u32.into())?;

		T::VestingBlockProvider::current_block_number();

		Pallet::<T>::on_finalize(T::BlockNumber::one());

	}:  _(RawOrigin::Signed(caller.clone()), caller.clone(), relay_account.into(), signature)
	verify {
		assert_eq!(Pallet::<T>::accounts_payable(&caller).unwrap().total_reward, (100u32.into()));
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
}

impl_benchmark_test_suite!(
	Pallet,
	crate::benchmarks::tests::new_test_ext(),
	crate::mock::Test
);
