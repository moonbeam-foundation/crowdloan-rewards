// Copyright 2019-2022 PureStake Inc.
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

//! Test utilities
use crate::{self as pallet_crowdloan_rewards, Config};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{ConstU32, GenesisBuild, Nothing, OnFinalize, OnInitialize},
};
use frame_system::EnsureSigned;
use sp_core::{ed25519, Pair, H256};
use sp_io;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};
use sp_std::convert::{From, TryInto};

pub type Balance = u128;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Crowdloan: pallet_crowdloan_rewards::{Pallet, Call, Storage, Event<T>},
		Utility: pallet_utility::{Pallet, Call, Storage, Event},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Test {
	type BaseCallFilter = Nothing;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type Index = u64;
	type RuntimeCall = RuntimeCall;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type OnSetCode = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 1;
}

impl pallet_balances::Config for Test {
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type MaxLocks = ();
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type HoldIdentifier = ();
	type FreezeIdentifier = ();
	type MaxHolds = ();
	type MaxFreezes = ();
}

pub struct MockedBlockProvider;
impl sp_runtime::traits::BlockNumberProvider for MockedBlockProvider {
	type BlockNumber = u64;

	fn current_block_number() -> Self::BlockNumber {
		System::current_block_number().saturating_add(1)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_block_number(_block: Self::BlockNumber) {}
}

parameter_types! {
	pub const TestMaxInitContributors: u32 = 8;
	pub const TestMinimumReward: u128 = 0;
	pub const TestInitialized: bool = false;
	pub const TestInitializationPayment: Perbill = Perbill::from_percent(20);
	pub const TestRewardAddressRelayVoteThreshold: Perbill = Perbill::from_percent(50);
	pub const TestSigantureNetworkIdentifier: &'static [u8] = b"test-";
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Initialized = TestInitialized;
	type InitializationPayment = TestInitializationPayment;
	type MaxInitContributors = TestMaxInitContributors;
	type MinimumReward = TestMinimumReward;
	type RewardCurrency = Balances;
	type RelayChainAccountId = [u8; 32];
	type RewardAddressRelayVoteThreshold = TestRewardAddressRelayVoteThreshold;
	// The origin that is allowed to associate the reward
	type RewardAddressAssociateOrigin = EnsureSigned<Self::AccountId>;
	// The origin that is allowed to change the reward
	type RewardAddressChangeOrigin = EnsureSigned<Self::AccountId>;
	type SignatureNetworkIdentifier = TestSigantureNetworkIdentifier;

	type VestingBlockNumber = u64;
	type VestingBlockProvider = MockedBlockProvider;
	type WeightInfo = ();
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WeightInfo = ();
	type PalletsOrigin = OriginCaller;
}

fn genesis(funded_amount: Balance) -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::default()
		.build_storage::<Test>()
		.unwrap();
	pallet_crowdloan_rewards::GenesisConfig::<Test> { funded_amount }
		.assimilate_storage(&mut storage)
		.expect("Pallet balances storage can be assimilated");

	let mut ext = sp_io::TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub type UtilityCall = pallet_utility::Call<Test>;

pub(crate) fn get_ed25519_pairs(num: u32) -> Vec<ed25519::Pair> {
	let seed: u128 = 12345678901234567890123456789012;
	let mut pairs = Vec::new();
	for i in 0..num {
		pairs.push(ed25519::Pair::from_seed(
			(seed.clone() + i as u128)
				.to_string()
				.as_bytes()
				.try_into()
				.unwrap(),
		))
	}
	pairs
}

pub(crate) fn empty() -> sp_io::TestExternalities {
	genesis(2500u32.into())
}

pub(crate) fn events() -> Vec<super::Event<Test>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let RuntimeEvent::Crowdloan(inner) = e {
				Some(inner)
			} else {
				None
			}
		})
		.collect::<Vec<_>>()
}

pub(crate) fn batch_events() -> Vec<pallet_utility::Event> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let RuntimeEvent::Utility(inner) = e {
				Some(inner)
			} else {
				None
			}
		})
		.collect::<Vec<_>>()
}

pub(crate) fn roll_to(n: u64) {
	let mut current_block_number = System::block_number();
	while current_block_number < n {
		Crowdloan::on_initialize(System::block_number());
		System::set_block_number(current_block_number);
		Crowdloan::on_finalize(System::block_number());
		current_block_number = current_block_number.saturating_add(1);
	}
}
