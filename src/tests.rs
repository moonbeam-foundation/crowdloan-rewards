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

//! Unit testing
use crate::*;
use frame_support::{assert_noop, assert_ok};
use mock::*;
#[test]
fn geneses() {
	two_assigned().execute_with(|| {
		assert!(System::events().is_empty());
		// accounts_payable
		assert!(Crowdloan::accounts_payable(&1).is_some());
		assert!(Crowdloan::accounts_payable(&2).is_some());
		assert!(Crowdloan::accounts_payable(&3).is_none());
		assert!(Crowdloan::accounts_payable(&4).is_none());
		assert!(Crowdloan::accounts_payable(&5).is_none());

		// claimed address existence
		assert!(Crowdloan::claimed_relay_chain_ids(&[1u8; 32]).is_some());
		assert!(Crowdloan::claimed_relay_chain_ids(&[2u8; 32]).is_some());
		assert!(Crowdloan::claimed_relay_chain_ids(&[3u8; 32]).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(&[4u8; 32]).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(&[5u8; 32]).is_none());
	});
}
#[test]
fn paying_works() {
	two_assigned().execute_with(|| {
		// 1 is payable
		assert!(Crowdloan::accounts_payable(&1).is_some());
		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 248);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(3)),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(5);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 5u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 310);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 372);
		roll_to(7);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 7u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 434);
		roll_to(230);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 500);
		roll_to(330);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(1)),
			Error::<Test>::RewardsAlreadyClaimed
		);

		let expected = vec![
			crate::Event::RewardsPaid(1, 248),
			crate::Event::RewardsPaid(1, 62),
			crate::Event::RewardsPaid(1, 62),
			crate::Event::RewardsPaid(1, 62),
			crate::Event::RewardsPaid(1, 66),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn update_address_works() {
	two_assigned().execute_with(|| {
		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(8)),
			Error::<Test>::NoAssociatedClaim
		);
		assert_ok!(Crowdloan::update_reward_address(Origin::signed(1), 8));
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().claimed_reward, 248);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(8)));
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().claimed_reward, 372);
		let expected = vec![
			crate::Event::RewardsPaid(1, 248),
			crate::Event::RewardAddressUpdated(1, 8),
			crate::Event::RewardsPaid(8, 124),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn update_address_with_existing_address_works() {
	two_assigned().execute_with(|| {
		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(2)));
		assert_ok!(Crowdloan::update_reward_address(Origin::signed(1), 2));
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().claimed_reward, 496);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(1)),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(2)));
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().claimed_reward, 746);
		let expected = vec![
			crate::Event::RewardsPaid(1, 248),
			crate::Event::RewardsPaid(2, 248),
			crate::Event::RewardAddressUpdated(1, 2),
			crate::Event::RewardsPaid(2, 250),
		];
		assert_eq!(events(), expected);
	});
}
