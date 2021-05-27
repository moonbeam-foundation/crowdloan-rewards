#![cfg(feature = "runtime-benchmarks")]

use crate::{BalanceOf, Call, Config, Pallet};
use cumulus_pallet_parachain_system::Pallet as RelayPallet;
use cumulus_primitives_core::relay_chain::v1::HeadData;
use cumulus_primitives_core::relay_chain::BlockNumber as RelayChainBlockNumber;
use cumulus_primitives_core::PersistedValidationData;
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelist_account};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_support::inherent::InherentData;
use frame_support::inherent::ProvideInherent;
use frame_support::traits::{Currency, Get}; // OnInitialize, OnFinalize
use frame_support::traits::{OnFinalize, OnInitialize};
use frame_system::RawOrigin;
use sp_core::crypto::{AccountId32, UncheckedFrom};
use sp_core::{ed25519};
use sp_core::H256;
use sp_runtime::traits::One;
use sp_runtime::{MultiSignature, MultiSigner};
use sp_std::convert::{TryInto, TryFrom};
use sp_std::vec;
use sp_std::vec::Vec;
use sp_trie::StorageProof;
use sp_io::crypto::{ed25519_sign, ed25519_generate};
use parity_scale_codec::Encode;
use ed25519_dalek::Signer;

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

fn create_fake_valid_proof() -> (H256, StorageProof) {
	let proof = StorageProof::new(vec![vec![
		127, 1, 6, 222, 61, 138, 84, 210, 126, 68, 169, 213, 206, 24, 150, 24, 242, 45, 180, 180,
		157, 149, 50, 13, 144, 33, 153, 76, 133, 15, 37, 184, 227, 133, 144, 0, 0, 32, 0, 0, 0, 16,
		0, 8, 0, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 5, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 6, 0, 0, 0,
	]]);
	let hash = [
		216, 6, 227, 175, 180, 211, 98, 117, 202, 245, 206, 51, 21, 143, 100, 232, 96, 217, 14, 71,
		243, 146, 7, 202, 245, 129, 165, 70, 72, 184, 130, 141,
	]
	.into();

	(hash, proof)
}

fn create_inherent_data<T: Config>(block_number: u32) -> InherentData {
	let (relay_parent_storage_root, relay_chain_state) = create_fake_valid_proof();

	let vfp = PersistedValidationData {
		relay_parent_number: block_number as RelayChainBlockNumber,
		relay_parent_storage_root,
		max_pov_size: 1000u32,
		parent_head: HeadData(vec![1, 1, 1]),
	};
	let inherent_data = {
		let mut inherent_data = InherentData::default();
		let system_inherent_data = ParachainInherentData {
			validation_data: vfp.clone(),
			relay_chain_state,
			downward_messages: Default::default(),
			horizontal_messages: Default::default(),
		};
		inherent_data
			.put_data(
				cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER,
				&system_inherent_data,
			)
			.expect("failed to put VFP inherent");
		inherent_data
	};
	inherent_data
}

/// Create a Contributor.
fn create_contributors<T: Config>(
	contributors: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)>,
	index: u32,
	limit: u32
) -> Result<(), &'static str> {
	Pallet::<T>::initialize_reward_vec(
		RawOrigin::Root.into(),
		contributors.clone(),
		index,
		limit,
	)?;
	Ok(())
}

fn crate_fake_sig<T: Config>(signed_account: T::AccountId) -> (AccountId32, MultiSignature) {
	let seed: [u8; 32] =  [
		47, 140, 97, 41, 216, 22, 207, 81, 195, 116, 188, 127, 8, 195, 230, 62, 209, 86, 207, 120,
		174, 251, 74, 101, 80, 217, 123, 135, 153, 121, 119, 238,
	];
	let secret  = ed25519_dalek::SecretKey::from_bytes(&seed).unwrap();
	let public = ed25519_dalek::PublicKey::from(&secret);
	let pair = ed25519_dalek::Keypair {secret, public};
	let sig = pair.sign(&signed_account.encode()).to_bytes();
	let signature: MultiSignature = ed25519::Signature::from_raw(sig).into();

	let ed_public : ed25519::Public = ed25519::Public::unchecked_from(public.to_bytes());
	let account: AccountId32 = ed_public.into();
	(account, signature.into())
}

pub fn create_ed25519_signature(payload: &[u8], pubkey: MultiSigner) -> MultiSignature {
	let edpubkey = ed25519::Public::try_from(pubkey).unwrap();
	let edsig = ed25519_sign(0.into(), &edpubkey, payload).unwrap();
	edsig.into()
}
pub fn create_ed25519_pubkey(seed: Vec<u8>) -> MultiSigner {
	ed25519_generate(0.into(), Some(seed)).into()
}

const MAX_ALREADY_USERS: u32 = 500;
const MAX_USERS: u32 = 500;
const SEED:u32 = 999999999;
benchmarks! {
	initialize_reward_vec {
		let x in 1..MAX_USERS;
		let y in 1..MAX_ALREADY_USERS;

		let total_pot = 100u32*(x+y);
		// Whats the worst case? the worst case is in which we have already N contributors
		// Fund pallet account
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());
		let mut contribution_vec = Vec::new();
		for i in 0..y{
			let seed = SEED - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
				account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
			let user = create_funded_user::<T>("user", seed, 0u32.into());
			let contribution: BalanceOf<T> = 100u32.into();
			contribution_vec.push((relay_chain_account.into(), Some(user.clone()), contribution));
			if i!=0 {
				whitelist_account!(user);
			}
		}
		create_contributors::<T>(contribution_vec, 0, x+y)?;
		RelayPallet::<T>::on_initialize(T::BlockNumber::one());
		let first_block_inherent = create_inherent_data::<T>(1u32);
		RelayPallet::<T>::create_inherent(&first_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");
		RelayPallet::<T>::on_finalize(T::BlockNumber::one());
		Pallet::<T>::on_finalize(T::BlockNumber::one());

		let mut contribution_vec = Vec::new();
		for i in 0..x{
			let seed = SEED - i -y;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
				account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
			let user = create_funded_user::<T>("user", seed, 0u32.into());
			let contribution: BalanceOf<T> = 100u32.into();
			contribution_vec.push((relay_chain_account.into(), Some(user.clone()), contribution));
			if i!=0 {
				whitelist_account!(user);
			}
		}

		let verifier = create_funded_user::<T>("user", SEED, 0u32.into());

	}:  _(RawOrigin::Root, contribution_vec, y, x+y)
	verify {
		assert!(Pallet::<T>::accounts_payable(&verifier).is_some());
		assert!(Pallet::<T>::initialized());
	}

show_me_the_money {
		let x in 1..MAX_USERS;
		// Fund pallet account
		let total_pot = 100u32*x;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());
		let mut contribution_vec = Vec::new();
		for i in 0..x{
			let seed = SEED - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
				account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
			let user = create_funded_user::<T>("user", seed, 0u32.into());
			let contribution: BalanceOf<T> = 100u32.into();
			contribution_vec.push((relay_chain_account.into(), Some(user.clone()), contribution));
			if i!=0 {
				whitelist_account!(user);
			}
		}
		create_contributors::<T>(contribution_vec.clone(), 0, contribution_vec.len() as u32)?;
		let caller: T::AccountId = create_funded_user::<T>("user", SEED, 100u32.into());
		let first_block_inherent = create_inherent_data::<T>(1u32);
		RelayPallet::<T>::on_initialize(T::BlockNumber::one());
		RelayPallet::<T>::create_inherent(&first_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");
		RelayPallet::<T>::on_finalize(T::BlockNumber::one());
		Pallet::<T>::on_finalize(T::BlockNumber::one());

		RelayPallet::<T>::on_initialize(4u32.into());

		let last_block_inherent = create_inherent_data::<T>(4u32);
		RelayPallet::<T>::create_inherent(&last_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");

		RelayPallet::<T>::on_finalize(4u32.into());

	}:  _(RawOrigin::Signed(caller.clone()))
	verify {
	  assert_eq!(Pallet::<T>::accounts_payable(&caller).unwrap().total_reward, (100u32.into()));
	}

	update_reward_address {
		let x in 3..MAX_USERS;
		// Fund pallet account
		let total_pot = 100u32*x;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());
		let mut contribution_vec = Vec::new();
		for i in 0..x{
			let seed = SEED - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
				account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
			let user = create_funded_user::<T>("user", seed, 0u32.into());
			let contribution: BalanceOf<T> = 100u32.into();
			contribution_vec.push((relay_chain_account.into(), Some(user.clone()), contribution));
			if i!=0 {
				whitelist_account!(user);
			}
		}
		create_contributors::<T>(contribution_vec.clone(), 0, contribution_vec.len() as u32)?;
		let caller: T::AccountId = create_funded_user::<T>("user", SEED, 100u32.into());
		let first_block_inherent = create_inherent_data::<T>(1u32);
		RelayPallet::<T>::on_initialize(T::BlockNumber::one());
		RelayPallet::<T>::create_inherent(&first_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");
		RelayPallet::<T>::on_finalize(T::BlockNumber::one());
		Pallet::<T>::on_finalize(T::BlockNumber::one());

		RelayPallet::<T>::on_initialize(10u32.into());

		let last_block_inherent = create_inherent_data::<T>(10u32);
		RelayPallet::<T>::create_inherent(&last_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");

		RelayPallet::<T>::on_finalize(10u32.into());
		let new_user = create_funded_user::<T>("user", SEED+1, 0u32.into());

	}:  _(RawOrigin::Signed(caller.clone()), new_user.clone())
	verify {
		assert_eq!(Pallet::<T>::accounts_payable(&new_user).unwrap().total_reward, (100u32.into()));
	}

	associate_native_identity {
		let x in 2..MAX_USERS;
		// Fund pallet account
		let total_pot = 100u32*x;
		fund_specific_account::<T>(Pallet::<T>::account_id(), total_pot.into());
		let mut contribution_vec = Vec::new();
		for i in 1..x{
			let seed = MAX_USERS - i;
			let mut account: [u8; 32] = [0u8; 32];
			let seed_as_slice = seed.to_be_bytes();
			for j in 0..seed_as_slice.len() {
				account[j] = seed_as_slice[j]
			}
			let relay_chain_account: AccountId32 = account.into();
			let user = create_funded_user::<T>("user", seed, 0u32.into());
			let contribution: BalanceOf<T> = 100u32.into();
			contribution_vec.push((relay_chain_account.into(), None, contribution));
			if i!=0 {
				whitelist_account!(user);
			}
		}



		let caller: T::AccountId = create_funded_user::<T>("user", MAX_USERS, 100u32.into());
		let (relay_account, signature) = crate_fake_sig::<T>(caller.clone());

		contribution_vec.push((relay_account.clone().into(), None, 100u32.into()));

		create_contributors::<T>(contribution_vec.clone(), 0, contribution_vec.len() as u32)?;
		let first_block_inherent = create_inherent_data::<T>(1u32);
		RelayPallet::<T>::on_initialize(T::BlockNumber::one());
		RelayPallet::<T>::create_inherent(&first_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");
		RelayPallet::<T>::on_finalize(T::BlockNumber::one());
		Pallet::<T>::on_finalize(T::BlockNumber::one());

		RelayPallet::<T>::on_initialize(10u32.into());

		let last_block_inherent = create_inherent_data::<T>(10u32);
		RelayPallet::<T>::create_inherent(&last_block_inherent)
			.expect("got an inherent")
			.dispatch_bypass_filter(RawOrigin::None.into())
			.expect("dispatch succeeded");

		RelayPallet::<T>::on_finalize(10u32.into());
		let new_user = create_funded_user::<T>("user", MAX_USERS+1, 0u32.into());
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
	#[test]
	fn update_reward_address() {
		new_test_ext().execute_with(|| {
			assert_ok!(test_benchmark_update_reward_address::<Test>());
		});
	}
	#[test]
	fn associate_native_identity() {
		new_test_ext().execute_with(|| {
			assert_ok!(test_benchmark_associate_native_identity::<Test>());
		});
	}
}

impl_benchmark_test_suite!(
	Pallet,
	crate::benchmarks::tests::new_test_ext(),
	crate::mock::Test
);
