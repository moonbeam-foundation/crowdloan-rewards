// Copyright 2019-2020 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

//! # Crowdloan Rewards Pallet
//!
//! This pallet issues rewards to citizens who participated in a crowdloan on the backing relay
//! chain (eg Kusama) in order to help this parrachain acquire a parachain slot.
//!
//! ## Monetary Policy
//!
//! This is simple and mock for now. We can do whatever we want.
//! This pallet stores a constant  "reward ratio" which is the number of reward tokens to pay per
//! contributed token. In simple cases this can be 1, but needs to be customizeable to allow for
//! vastly differing absolute token supplies between relay and para.
//! Vesting is also linear. No tokens are vested at genesis and they unlock linearly until a
//! predecided block number. Vesting computations happen on demand when payouts are requested. So
//! no block weight is ever wasted on this, and there is no "base-line" cost of updating vestings.
//! Like I said, we can anything we want there. Even a non-linear reward curve to disincentivize
//! whales.
//!
//! ## Payout Mechanism
//!
//! The current payout mechanism requires contributors to claim their payouts. Because they are
//! paying the transaction fees for this themselves, they can do it as often as every block, or
//! wait and claim the entire thing once it is fully vested. We could consider auto payouts if we
//! want.
//!
//! ## Sourcing Contribution Information
//!
//! The pallet can learn about the crowdloan contributions in several ways.
//!
//! * **Through the initialize_reward_vec extrinsic*
//!
//! The simplest way is to call the initialize_reward_vec through a democracy proposal/sudo call.
//! This makes sense in a scenario where the crowdloan took place entirely offchain.
//! This extrinsic initializes the associated and unassociated stoerage with the provided data
//!
//! * **ReadingRelayState**
//!
//! The most elegant, but most complex solution would be for the para to read the contributions
//! directly from the relay state. Blocked by https://github.com/paritytech/cumulus/issues/320 so
//! I won't pursue it further right now. I can't decide whether that would really add security /
//! trustlessness, or is just a sexy blockchain thing to do. Contributors can always audit the
//! democracy proposal and make sure their contribution is in it, so in that sense reading relay state
//! isn't necessary. But if a single contribution is left out, the rest of the contributors might
//! not care enough to delay network launch. The little guy might get censored.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::pallet;
pub use pallet::*;
#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

#[pallet]
pub mod pallet {

	use frame_support::{
		dispatch::fmt::Debug,
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement::KeepAlive},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_core::crypto::AccountId32;
	use sp_runtime::traits::{AccountIdConversion, Saturating, Verify};
	use sp_runtime::{MultiSignature, Perbill, SaturatedConversion};
	use sp_std::{convert::TryInto, vec::Vec};

	/// The Author Filter pallet
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	pub const PALLET_ID: PalletId = PalletId(*b"Crowdloa");

	pub struct RelayChainBeacon<T>(PhantomData<T>);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: cumulus_pallet_parachain_system::Config + frame_system::Config {
		/// The overarching event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Checker for the reward vec, is it initalized already?
		type Initialized: Get<bool>;
		/// Percentage to be payed at initialization
		type InitializationPayment: Get<Perbill>;
		/// The minimum contribution to which rewards will be paid.
		type MinimumReward: Get<BalanceOf<Self>>;
		/// The currency in which the rewards will be paid (probably the parachain native currency)
		type RewardCurrency: Currency<Self::AccountId>;
		// TODO What trait bounds do I need here? I think concretely we would
		// be using MultiSigner? Or maybe MultiAccount? I copied these from frame_system
		/// The AccountId type contributors used on the relay chain.
		type RelayChainAccountId: Parameter
			+ Member
			+ MaybeSerializeDeserialize
			+ Ord
			+ Default
			+ Debug
			+ Into<AccountId32>;

		/// The total vesting period. Ideally this should be less than the lease period to ensure
		/// there is no overlap between contributors from two different auctions
		type VestingPeriod: Get<Self::BlockNumber>;
	}

	type BalanceOf<T> = <<T as Config>::RewardCurrency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	/// Stores info about the rewards owed as well as how much has been vested so far.
	/// For a primer on this kind of design, see the recipe on compounding interest
	/// https://substrate.dev/recipes/fixed-point.html#continuously-compounding
	#[derive(Default, Clone, Encode, Decode, RuntimeDebug)]
	pub struct RewardInfo<T: Config> {
		pub total_reward: BalanceOf<T>,
		pub claimed_reward: BalanceOf<T>,
		pub last_paid: T::BlockNumber,
		pub free_claim_done: bool,
	}

	// This hook is in charge of initializing the relay chain height at the first block of the parachain
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(n: T::BlockNumber) {
			// In the first block of the parachain we need to introduce the relay block related info
			if n == 1u32.into() {
				let slot = cumulus_pallet_parachain_system::Module::<T>::validation_data()
					.expect("validation data was set in parachain system inherent")
					.relay_parent_number;
				<InitRelayBlock<T>>::put(slot);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Associate a native rewards_destination identity with a crowdloan contribution.
		///
		/// This is an unsigned call because the caller may not have any funds to pay fees with.
		/// This is inspired by Polkadot's claims pallet:
		/// https://github.com/paritytech/polkadot/blob/master/runtime/common/src/claims.rs
		///
		/// The contributor needs to issue an additional addmemo transaction if it wants to receive
		/// the reward in a parachain native account. For the moment I will leave this function here
		/// just in case the contributor forgot to add such a memo field. Whenever we can read the
		/// state of the relay chain, we should first check whether that memo field exists in the
		/// contribution
		#[pallet::weight(0)]
		pub fn associate_native_identity(
			origin: OriginFor<T>,
			reward_account: T::AccountId,
			relay_account: T::RelayChainAccountId,
			proof: MultiSignature,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;

			// Check the proof:
			// 1. Is signed by an actual unassociated contributor
			// 2. Signs a valid native identity
			// Check the proof. The Proof consists of a Signature of the rewarded account with the
			// claimer key
			let payload = reward_account.encode();
			ensure!(
				proof.verify(payload.as_slice(), &relay_account.clone().into()),
				Error::<T>::InvalidClaimSignature
			);

			// We ensure the relay chain id wast not yet associated to avoid multi-claiming
			ensure!(
				ClaimedRelayChainIds::<T>::get(&relay_account).is_none(),
				Error::<T>::AlreadyAssociated
			);

			// Upon error this should check the relay chain state in this case
			let mut reward_info = UnassociatedContributions::<T>::get(&relay_account)
				.ok_or(Error::<T>::NoAssociatedClaim)?;

			// Make the first payment
			let first_payment = T::InitializationPayment::get() * reward_info.total_reward;

			T::RewardCurrency::transfer(
				&PALLET_ID.into_account(),
				&reward_account,
				first_payment,
				KeepAlive,
			)?;

			Self::deposit_event(Event::InitialPaymentMade(
				reward_account.clone(),
				first_payment,
			));

			reward_info.claimed_reward = first_payment;

			// Insert on payable
			AccountsPayable::<T>::insert(&reward_account, &reward_info);

			// Remove from unassociated
			<UnassociatedContributions<T>>::remove(&relay_account);

			// Insert in mapping
			ClaimedRelayChainIds::<T>::insert(&relay_account, ());

			// Emit Event
			Self::deposit_event(Event::NativeIdentityAssociated(
				relay_account,
				reward_account,
				reward_info.total_reward,
			));

			Ok(Default::default())
		}

		/// First claim for collecting vested tokens. This one is free
		#[pallet::weight((0, DispatchClass::Normal, Pays::No))]
		pub fn my_first_claim(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let payee = ensure_signed(origin)?;

			// Calculate the vested amount on demand.
			let info = AccountsPayable::<T>::get(&payee).ok_or(Error::<T>::NoAssociatedClaim)?;

			ensure!(
				info.free_claim_done == false,
				Error::<T>::FirstClaimAlreadyDone
			);
			ensure!(
				info.claimed_reward < info.total_reward,
				Error::<T>::RewardsAlreadyClaimed
			);

			Self::make_vested_payment(info, payee.clone())
		}

		/// Collect whatever portion of your reward are currently vested.
		#[pallet::weight(0)]
		pub fn show_me_the_money(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let payee = ensure_signed(origin)?;

			// Calculate the veted amount on demand.
			let info = AccountsPayable::<T>::get(&payee).ok_or(Error::<T>::NoAssociatedClaim)?;
			ensure!(
				info.claimed_reward < info.total_reward,
				Error::<T>::RewardsAlreadyClaimed
			);

			Self::make_vested_payment(info, payee.clone())
		}

		/// Update reward address. To determine whether its something we want to keep
		#[pallet::weight(0)]
		pub fn update_reward_address(
			origin: OriginFor<T>,
			new_reward_account: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let signer = ensure_signed(origin)?;

			// Calculate the veted amount on demand.
			let mut info =
				AccountsPayable::<T>::get(&signer).ok_or(Error::<T>::NoAssociatedClaim)?;

			if let Some(info_existing_account) = AccountsPayable::<T>::get(&new_reward_account) {
				info.total_reward = info
					.total_reward
					.saturating_add(info_existing_account.total_reward);
				info.claimed_reward = info
					.claimed_reward
					.saturating_add(info_existing_account.claimed_reward);
			}

			// Remove previous rewarded account
			AccountsPayable::<T>::remove(&signer);

			// Update new rewarded acount
			AccountsPayable::<T>::insert(&new_reward_account, &info);

			// Emit event
			Self::deposit_event(Event::RewardAddressUpdated(signer, new_reward_account));

			Ok(Default::default())
		}

		/// Initialize the reward distribution storage. It shortcuts whenever an error is found
		/// We can change this behavior to check this beforehand if we prefer
		/// We only set this to "initialized" once we receive index==limit
		/// This is expected to be executed with batch_all, that atomically initializes contributions
		/// TODO Should we perform sanity checks here? (i.e., min contribution)
		#[pallet::weight(0)]
		pub fn initialize_reward_vec(
			origin: OriginFor<T>,
			rewards: Vec<(T::RelayChainAccountId, Option<T::AccountId>, BalanceOf<T>)>,
			index: u32,
			limit: u32,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			//let now = frame_system::Pallet::<T>::block_number();
			let initialized = <Initialized<T>>::get();
			ensure!(
				initialized == false,
				Error::<T>::RewardVecAlreadyInitialized
			);

			for (relay_account, native_account, reward) in &rewards {
				if ClaimedRelayChainIds::<T>::get(&relay_account).is_some()
					|| UnassociatedContributions::<T>::get(&relay_account).is_some()
				{
					// Dont fail as this is supposed to be called with batch calls and we
					// dont want to stall the rest of the contributions
					Self::deposit_event(Event::InitializedAlreadyInitializedAccount(
						relay_account.clone(),
						native_account.clone(),
						*reward,
					));
					continue;
				}

				if *reward < T::MinimumReward::get() {
					// Dont fail as this is supposed to be called with batch calls and we
					// dont want to stall the rest of the contributions
					Self::deposit_event(Event::InitializedAccountWithNotEnoughContribution(
						relay_account.clone(),
						native_account.clone(),
						*reward,
					));
					continue;
				}

				// If we have a native_account, we make the payment
				let initial_payment = if let Some(native_account) = native_account {
					let first_payment = T::InitializationPayment::get() * (*reward);
					T::RewardCurrency::transfer(
						&PALLET_ID.into_account(),
						&native_account,
						first_payment,
						KeepAlive,
					)?;
					Self::deposit_event(Event::InitialPaymentMade(
						native_account.clone(),
						first_payment,
					));
					first_payment
				} else {
					0u32.into()
				};

				// We need to calculate the vesting based on the relay block number
				let reward_info = RewardInfo {
					total_reward: *reward,
					claimed_reward: initial_payment,
					last_paid: <InitRelayBlock<T>>::get().into(),
					free_claim_done: false,
				};

				if let Some(native_account) = native_account {
					AccountsPayable::<T>::insert(native_account, reward_info);
					ClaimedRelayChainIds::<T>::insert(relay_account, ());
				} else {
					UnassociatedContributions::<T>::insert(relay_account, reward_info);
				}
			}
			// Let's ensure we can close initialization
			if index + rewards.len() as u32 == limit {
				let claimed_rewards = AccountsPayable::<T>::iter().fold(
					0u32.into(),
					|acc: BalanceOf<T>, (_, reward_info)| {
						acc + reward_info.total_reward - reward_info.claimed_reward
					},
				);
				let unassociated_rewards = UnassociatedContributions::<T>::iter()
					.fold(0u32.into(), |acc: BalanceOf<T>, (_, reward_info)| {
						acc + reward_info.total_reward
					});
				ensure!(
					claimed_rewards + unassociated_rewards == Self::pot(),
					Error::<T>::RewardsDoNotMatchFund
				);
				<Initialized<T>>::put(true);
			}
			Ok(Default::default())
		}
	}

	/// Converts the given binary data into ASCII-encoded hex. It will be twice the length.
	pub fn to_ascii_hex(data: &[u8]) -> Vec<u8> {
		let mut r = Vec::with_capacity(data.len() * 2);
		let mut push_nibble = |n| r.push(if n < 10 { b'0' + n } else { b'a' - 10 + n });
		for &b in data.iter() {
			push_nibble(b / 16);
			push_nibble(b % 16);
		}
		r
	}

	impl<T: Config> Pallet<T> {
		/// The account ID that holds the Crowdloan's funds
		pub fn account_id() -> T::AccountId {
			PALLET_ID.into_account()
		}
		/// The Account Id's balance
		fn pot() -> BalanceOf<T> {
			T::RewardCurrency::free_balance(&Self::account_id())
		}
		fn make_vested_payment(
			mut info: RewardInfo<T>,
			payee: T::AccountId,
		) -> DispatchResultWithPostInfo {
			// Vesting is done in relation with the relay chain slot
			let now: T::BlockNumber =
				cumulus_pallet_parachain_system::Module::<T>::validation_data()
					.expect("validation data was set in parachain system inherent")
					.relay_parent_number
					.into();

			// Substract the first payment from the vested amount
			let first_paid = T::InitializationPayment::get() * info.total_reward;

			let payable_per_block = (info.total_reward - first_paid)
				/ T::VestingPeriod::get()
					.saturated_into::<u128>()
					.try_into()
					.ok()
					.ok_or(Error::<T>::WrongConversionU128ToBalance)?; //TODO safe math;

			let payable_period = now.saturating_sub(info.last_paid);

			let pay_period_as_balance: BalanceOf<T> = payable_period
				.saturated_into::<u128>()
				.try_into()
				.ok()
				.ok_or(Error::<T>::WrongConversionU128ToBalance)?;

			// If the period is bigger than whats missing to pay, then return whats missing to pay
			let payable_amount = if pay_period_as_balance.saturating_mul(payable_per_block)
				< info.total_reward.saturating_sub(info.claimed_reward)
			{
				pay_period_as_balance.saturating_mul(payable_per_block)
			} else {
				info.total_reward.saturating_sub(info.claimed_reward)
			};

			// Update the stored info
			info.last_paid = now;
			info.free_claim_done = true;
			info.claimed_reward = info.claimed_reward.saturating_add(payable_amount);
			AccountsPayable::<T>::insert(&payee, &info);

			// This pallet controls an amount of funds and transfers them to each of the contributors
			//TODO: contributors should have the balance locked for tranfers but not for democracy
			T::RewardCurrency::transfer(
				&PALLET_ID.into_account(),
				&payee,
				payable_amount,
				KeepAlive,
			)?;
			// Emit event
			Self::deposit_event(Event::RewardsPaid(payee, payable_amount));
			Ok(Default::default())
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// User trying to associate a native identity with a relay chain identity for posterior
		/// reward claiming provided an already associated relay chain identity
		AlreadyAssociated,
		/// First claim already done
		FirstClaimAlreadyDone,
		/// The contribution is not high enough to be eligible for rewards
		RewardNotHighEnough,
		/// User trying to associate a native identity with a relay chain identity for posterior
		/// reward claiming provided a wrong signature
		InvalidClaimSignature,
		/// User trying to claim the first free reward provided the wrong signature
		InvalidFreeClaimSignature,
		/// User trying to claim an award did not have an claim associated with it. This may mean
		/// they did not contribute to the crowdloan, or they have not yet associated a native id
		/// with their contribution
		NoAssociatedClaim,
		/// User trying to claim rewards has already claimed all rewards associated with its
		/// identity and contribution
		RewardsAlreadyClaimed,
		/// Reward vec has already been initialized
		RewardVecAlreadyInitialized,
		/// Reward vec has already been initialized
		RewardsDoNotMatchFund,
		/// Invalid conversion while calculating payable amount
		WrongConversionU128ToBalance,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// The amount of funds this pallet controls
		pub funded_amount: BalanceOf<T>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				funded_amount: 1u32.into(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		// This sets the funds of the crowdloan pallet
		fn build(&self) {
			T::RewardCurrency::deposit_creating(&Pallet::<T>::account_id(), self.funded_amount);
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn accounts_payable)]
	pub type AccountsPayable<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, RewardInfo<T>>;
	#[pallet::storage]
	#[pallet::getter(fn claimed_relay_chain_ids)]
	pub type ClaimedRelayChainIds<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RelayChainAccountId, ()>;
	#[pallet::storage]
	#[pallet::getter(fn unassociated_contributions)]
	pub type UnassociatedContributions<T: Config> =
		StorageMap<_, Blake2_128Concat, T::RelayChainAccountId, RewardInfo<T>>;
	#[pallet::storage]
	#[pallet::getter(fn initialized)]
	pub type Initialized<T: Config> = StorageValue<_, bool, ValueQuery, T::Initialized>;

	#[pallet::storage]
	#[pallet::getter(fn init_relay_block)]
	/// Relay block height at the initialization of the pallet
	type InitRelayBlock<T: Config> = StorageValue<_, u32, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// The initial payment of InitializationPayment % was paid
		InitialPaymentMade(T::AccountId, BalanceOf<T>),
		/// Someone has proven they made a contribution and associated a native identity with it.
		/// Data is the relay account,  native account and the total amount of _rewards_ that will be paid
		NativeIdentityAssociated(T::RelayChainAccountId, T::AccountId, BalanceOf<T>),
		/// A contributor has claimed some rewards.
		/// Data is the account getting paid and the amount of rewards paid.
		RewardsPaid(T::AccountId, BalanceOf<T>),
		/// A contributor has updated the reward address.
		RewardAddressUpdated(T::AccountId, T::AccountId),
		/// When initializing the reward vec an already initialized account was found
		InitializedAlreadyInitializedAccount(
			T::RelayChainAccountId,
			Option<T::AccountId>,
			BalanceOf<T>,
		),
		/// When initializing the reward vec an already initialized account was found
		InitializedAccountWithNotEnoughContribution(
			T::RelayChainAccountId,
			Option<T::AccountId>,
			BalanceOf<T>,
		),
	}
}
