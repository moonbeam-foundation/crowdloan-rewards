// Copyright 2019-2021 PureStake Inc.
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
	traits::{GenesisBuild, OnFinalize, OnInitialize},
};
use sp_core::{ed25519, Pair, H256};
use sp_io;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};
use sp_std::convert::From;
use sp_std::convert::TryInto;

pub type AccountId = u64;
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
		Crowdloan: pallet_crowdloan_rewards::{Pallet, Call, Storage, Config<T>, Event<T>},
		Utility: pallet_utility::{Pallet, Call, Storage, Event},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(1024);
}

impl frame_system::Config for Test {
	type BaseCallFilter = ();
	type BlockWeights = ();
	type BlockLength = ();
	type Origin = Origin;
	type Index = u64;
	type Call = Call;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 1;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type Balance = Balance;
	type Event = Event;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
}

parameter_types! {
	pub const TestVestingPeriod: u64 = 8;
}

impl Config for Test {
	type Event = Event;
	type RewardCurrency = Balances;
	type RelayChainAccountId = [u8; 32];
	type VestingPeriod = TestVestingPeriod;
}

impl pallet_utility::Config for Test {
	type Event = Event;
	type Call = Call;
	type WeightInfo = ();
}

fn genesis(
	assigned: Vec<([u8; 32], AccountId, u32)>,
	unassigned: Vec<([u8; 32], u32)>,
) -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::default()
		.build_storage::<Test>()
		.unwrap();

	pallet_crowdloan_rewards::GenesisConfig::<Test> {
		associated: assigned,
		unassociated: unassigned,
		reward_ratio: 1,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

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

pub(crate) fn two_assigned_three_unassigned() -> sp_io::TestExternalities {
	let pairs = get_ed25519_pairs(3);
	genesis(
		vec![
			// validators
			([1u8; 32].into(), 1, 500),
			([2u8; 32].into(), 2, 500),
		],
		vec![
			// validators
			(pairs[0].public().into(), 500),
			(pairs[1].public().into(), 500),
			(pairs[2].public().into(), 500),
		],
	)
}

pub(crate) fn events() -> Vec<super::Event<Test>> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| {
			if let Event::pallet_crowdloan_rewards(inner) = e {
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
			if let Event::pallet_utility(inner) = e {
				Some(inner)
			} else {
				None
			}
		})
		.collect::<Vec<_>>()
}

pub(crate) fn roll_to(n: u64) {
	while System::block_number() < n {
		Crowdloan::on_finalize(System::block_number());
		Balances::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		Balances::on_initialize(System::block_number());
		Crowdloan::on_initialize(System::block_number());
		Utility::on_initialize(System::block_number());
	}
}
