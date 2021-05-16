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
#[cfg(any(test, feature = "runtime-benchmarks"))]
mod benchmarks;
#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

#[pallet]
pub mod pallet {

	use frame_support::traits::ExistenceRequirement::KeepAlive;
	use frame_support::{dispatch::fmt::Debug, pallet_prelude::*, traits::Currency};
	use frame_system::pallet_prelude::*;
	use nimbus_primitives::SlotBeacon;
	use sp_core::crypto::AccountId32;
	use sp_runtime::traits::{Saturating, Verify};
	use sp_runtime::{MultiSignature, Perbill, SaturatedConversion};
	use sp_std::{convert::TryInto, vec::Vec};

	/// The Author Filter pallet
	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	pub struct RelayChainBeacon<T>(PhantomData<T>);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: cumulus_pallet_parachain_system::Config + frame_system::Config {
		/// Default number of blocks per round at genesis
		type DefaultBlocksPerRound: Get<u32>;
		/// The period after which the contribution storage can be initialized again
		type MinimumContribution: Get<BalanceOf<Self>>;
		/// The overarching event type
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		/// Checker for the reward vec, is it initalized already?
		type Initialized: Get<bool>;
		/// Percentage to be payed at initialization
		type InitializationPayment: Get<Perbill>;
		/// The account from which payments will be performed
		type PalletAccountId: Get<Self::AccountId>;
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
			+ Into<AccountId32>
			+ From<AccountId32>;

		/// The total vesting period. Ideally this should be less than the lease period to ensure
		/// there is no overlap between contributors from two different auctions
		type VestingPeriod: Get<Self::BlockNumber>;
	}
	type RoundIndex = u32;

	pub type BalanceOf<T> = <<T as Config>::RewardCurrency as Currency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

	#[derive(Copy, Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug)]
	/// The current round index and transition information
	pub struct RoundInfo<BlockNumber> {
		/// Current round index
		pub current: RoundIndex,
		/// The first block of the current round
		pub first: BlockNumber,
		/// The length of the current round in number of blocks
		pub length: u32,
	}
	impl<
			B: Copy
				+ sp_std::ops::Add<Output = B>
				+ sp_std::ops::Sub<Output = B>
				+ From<u32>
				+ PartialOrd,
		> RoundInfo<B>
	{
		pub fn new(current: RoundIndex, first: B, length: u32) -> RoundInfo<B> {
			RoundInfo {
				current,
				first,
				length,
			}
		}
		/// Check if the round should be updated
		pub fn should_update(&self, now: B) -> bool {
			now - self.first >= self.length.into()
		}
		/// New round
		pub fn update(&mut self, now: B) {
			self.current += 1u32;
			self.first = now;
		}
	}
	impl<
			B: Copy
				+ sp_std::ops::Add<Output = B>
				+ sp_std::ops::Sub<Output = B>
				+ From<u32>
				+ PartialOrd,
		> Default for RoundInfo<B>
	{
		fn default() -> RoundInfo<B> {
			RoundInfo::new(0u32, 0u32.into(), 1u32.into())
		}
	}

	/// Stores info about the rewards owed as well as how much has been vested so far.
	/// For a primer on this kind of design, see the recipe on compounding interest
	/// https://substrate.dev/recipes/fixed-point.html#continuously-compounding
	#[derive(Default, Clone, Encode, Decode, RuntimeDebug)]
	pub struct RewardInfo<T: Config> {
		pub total_reward: BalanceOf<T>,
		pub claimed_reward: BalanceOf<T>,
		pub last_paid: T::BlockNumber,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(n: T::BlockNumber) {
			let mut round = <Round<T>>::get();
			if round.should_update(n) {
				// mutate round
				round.update(n);
				// pay all stakers for T::BondDuration rounds ago
				Self::pay_contributors(n);
				<Round<T>>::put(round);
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
				&T::PalletAccountId::get(),
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

		/// Collect whatever portion of your reward are currently vested.
		#[pallet::weight(0)]
		pub fn show_me_the_money(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let payee = ensure_signed(origin)?;

			// Calculate the vested amount on demand.
			let mut info =
				AccountsPayable::<T>::get(&payee).ok_or(Error::<T>::NoAssociatedClaim)?;
			ensure!(
				info.claimed_reward < info.total_reward,
				Error::<T>::RewardsAlreadyClaimed
			);

			ensure!(
				info.total_reward > T::MinimumContribution::get(),
				Error::<T>::ContributionNotHighEnough
			);

			let now = frame_system::Pallet::<T>::block_number();

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
			info.claimed_reward = info.claimed_reward.saturating_add(payable_amount);
			AccountsPayable::<T>::insert(&payee, &info);

			// Make the payment
			// TODO where are these reward funds coming from? Currently I'm just minting them right here.
			// 1. We could have an associated type to absorb the imbalance.
			// 2. We could have this pallet control a pot of funds, and initialize it at genesis.
			T::RewardCurrency::transfer(
				&T::PalletAccountId::get(),
				&payee,
				payable_amount,
				KeepAlive,
			)?;

			//	T::RewardCurrency::deposit_creating(&payee, payable_amount);

			// Emit event
			Self::deposit_event(Event::RewardsPaid(payee, payable_amount));

			Ok(Default::default())
		}
		/// Collect whatever portion of your reward are currently vested.
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
		/// This function ensures that the current block number>=NextInitialization
		/// Also, updates NextInitialization when given index + len(contributors) = limit
		/// TODO Should we perform sanity checks here?
		#[pallet::weight(0)]
		pub fn initialize_reward_vec(
			origin: OriginFor<T>,
			contributions: Vec<(T::RelayChainAccountId, Option<T::AccountId>, u32)>,
			reward_ratio: u32,
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
			for (relay_account, native_account, contribution) in &contributions {
				if ClaimedRelayChainIds::<T>::get(&relay_account).is_some()
					|| UnassociatedContributions::<T>::get(&relay_account).is_some()
				{
					// Dont fail as this is supposed to be called with batch calls and we
					// dont want to stall the rest of the contributions
					Self::deposit_event(Event::ErrorWhileInitializing(
						relay_account.clone(),
						native_account.clone(),
						*contribution,
					));
					continue;
				}

				let total_payment = BalanceOf::<T>::from(*contribution)
					.saturating_mul(BalanceOf::<T>::from(reward_ratio));

				// If we have a native_account, we make the payment
				let initial_payment = if let Some(native_account) = native_account {
					let first_payment = T::InitializationPayment::get() * total_payment;
					T::RewardCurrency::transfer(
						&T::PalletAccountId::get(),
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

				let reward_info = RewardInfo {
					total_reward: total_payment,
					claimed_reward: initial_payment,
					last_paid: 0u32.into(),
				};

				if let Some(native_account) = native_account {
					AccountsPayable::<T>::insert(native_account, reward_info);
					ClaimedRelayChainIds::<T>::insert(relay_account, ());
				} else {
					UnassociatedContributions::<T>::insert(relay_account, reward_info);
				}
			}
			if index + contributions.len() as u32 == limit {
				<Initialized<T>>::put(true);
			}
			Ok(Default::default())
		}
	}

	impl<T: cumulus_pallet_parachain_system::Config> SlotBeacon for RelayChainBeacon<T> {
		fn slot() -> u32 {
			cumulus_pallet_parachain_system::Module::<T>::validation_data()
				.expect("validation data was set in parachain system inherent")
				.relay_parent_number
		}
	}

	impl<T: Config> Pallet<T> {
		fn pay_contributors(now: T::BlockNumber) {
			let enumerated: Vec<_> = AccountsPayable::<T>::iter().collect();
			for (payee, mut info) in enumerated {
				let payable_per_block = info.total_reward
					/ T::VestingPeriod::get()
						.saturated_into::<u128>()
						.try_into()
						.ok()
						.unwrap(); //TODO safe math;

				let payable_period = now.saturating_sub(info.last_paid);
				let pay_period_as_balance: BalanceOf<T> = payable_period
					.saturated_into::<u128>()
					.try_into()
					.ok()
					.unwrap();

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
				info.claimed_reward = info.claimed_reward.saturating_add(payable_amount);
				AccountsPayable::<T>::insert(&payee, &info);

				// Make the payment
				// TODO where are these reward funds coming from? Currently I'm just minting them right here.
				// 1. We could have an associated type to absorb the imbalance.
				// 2. We could have this pallet control a pot of funds, and initialize it at genesis.
				T::RewardCurrency::deposit_creating(&payee, payable_amount);

				// Emit event
				Self::deposit_event(Event::RewardsPaid(payee, payable_amount));
			}
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// User trying to associate a native identity with a relay chain identity for posterior
		/// reward claiming provided an already associated relay chain identity
		AlreadyAssociated,
		/// The contribution is not high enough to be eligible for rewards
		ContributionNotHighEnough,
		/// Current Lease Period has already been initialized
		CurrentLeasePeriodAlreadyInitialized,
		/// User trying to associate a native identity with a relay chain identity for posterior
		/// reward claiming provided a wrong signature
		InvalidClaimSignature,
		/// User trying to claim an award did not have an claim associated with it. This may mean
		/// they did not contribute to the crowdloan, or they have not yet associated a native id
		/// with their contribution
		NoAssociatedClaim,
		/// User trying to claim rewards has already claimed all rewards associated with its
		/// identity and contribution
		RewardsAlreadyClaimed,
		/// Reward vec has already been initialized
		RewardVecAlreadyInitialized,
		/// Invalid conversion while calculating payable amount
		WrongConversionU128ToBalance,
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Contributions that have a native account id associated already.
		pub associated: Vec<(T::RelayChainAccountId, T::AccountId, u32)>,
		/// Contributions that will need a native account id to be associated through an extrinsic.
		pub unassociated: Vec<(T::RelayChainAccountId, u32)>,
		/// The ratio of (reward tokens to be paid) / (relay chain funds contributed)
		/// This is dead stupid simple using a u32. So the reward amount has to be an integer
		/// multiple of the contribution amount. A better fixed-ratio solution would be
		/// https://crates.parity.io/sp_arithmetic/fixed_point/struct.FixedU128.html
		/// We could also do something fancy and non-linear if the need arises.
		pub reward_ratio: u32,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				associated: Vec::new(),
				unassociated: Vec::new(),
				reward_ratio: 1,
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			let round: RoundInfo<T::BlockNumber> =
				RoundInfo::new(1u32, 0u32.into(), T::DefaultBlocksPerRound::get());
			<Round<T>>::put(round);
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
	#[pallet::getter(fn round)]
	/// Current round index and next round scheduled transition
	type Round<T: Config> = StorageValue<_, RoundInfo<T::BlockNumber>, ValueQuery>;

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
		/// An error occurred when initializing the reward vector for a particular RelayChainAccount
		ErrorWhileInitializing(T::RelayChainAccountId, Option<T::AccountId>, u32),
	}
}
