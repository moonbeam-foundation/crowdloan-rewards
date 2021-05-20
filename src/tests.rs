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
use frame_support::dispatch::{DispatchError, Dispatchable};
use frame_support::{assert_noop, assert_ok};
use mock::*;
use parity_scale_codec::Encode;
use sp_core::Pair;
use sp_runtime::MultiSignature;

#[test]
fn geneses() {
	empty().execute_with(|| {
		assert!(System::events().is_empty());
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// accounts_payable
		assert!(Crowdloan::accounts_payable(&1).is_some());
		assert!(Crowdloan::accounts_payable(&2).is_some());
		assert!(Crowdloan::accounts_payable(&3).is_none());
		assert!(Crowdloan::accounts_payable(&4).is_none());
		assert!(Crowdloan::accounts_payable(&5).is_none());

		// claimed address existence
		assert!(Crowdloan::claimed_relay_chain_ids(&[1u8; 32]).is_some());
		assert!(Crowdloan::claimed_relay_chain_ids(&[2u8; 32]).is_some());
		assert!(Crowdloan::claimed_relay_chain_ids(pairs[0].public().as_array_ref()).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(pairs[1].public().as_array_ref()).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(pairs[2].public().as_array_ref()).is_none());

		// unassociated_contributions
		assert!(Crowdloan::unassociated_contributions(&[1u8; 32]).is_none());
		assert!(Crowdloan::unassociated_contributions(&[2u8; 32]).is_none());
		assert!(Crowdloan::unassociated_contributions(pairs[0].public().as_array_ref()).is_some());
		assert!(Crowdloan::unassociated_contributions(pairs[1].public().as_array_ref()).is_some());
		assert!(Crowdloan::unassociated_contributions(pairs[2].public().as_array_ref()).is_some());
	});
}

#[test]
fn proving_assignation_works() {
	let pairs = get_ed25519_pairs(3);
	let signature: MultiSignature = pairs[0].sign(&3u64.encode()).into();
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// 4 is not payable first
		assert!(Crowdloan::accounts_payable(&3).is_none());
		roll_to(4);
		// Signature is wrong, prove fails
		assert_noop!(
			Crowdloan::associate_native_identity(
				Origin::signed(4),
				4,
				pairs[0].public().into(),
				signature.clone()
			),
			Error::<Test>::InvalidClaimSignature
		);
		// Signature is right, prove passes
		assert_ok!(Crowdloan::associate_native_identity(
			Origin::signed(4),
			3,
			pairs[0].public().into(),
			signature.clone()
		));
		// Signature is right, but address already claimed
		assert_noop!(
			Crowdloan::associate_native_identity(
				Origin::signed(4),
				3,
				pairs[0].public().into(),
				signature
			),
			Error::<Test>::AlreadyAssociated
		);
		// now three is payable
		assert!(Crowdloan::accounts_payable(&3).is_some());
		assert!(Crowdloan::unassociated_contributions(pairs[0].public().as_array_ref()).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(pairs[0].public().as_array_ref()).is_some());

		let expected = vec![
			crate::Event::InitialPaymentMade(1, 100),
			crate::Event::InitialPaymentMade(2, 100),
			crate::Event::InitialPaymentMade(3, 100),
			crate::Event::NativeIdentityAssociated(pairs[0].public().into(), 3, 500),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn paying_works() {
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// 1 is payable
		assert!(Crowdloan::accounts_payable(&1).is_some());
		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 300);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(3)),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(5);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 5u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 350);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 400);
		roll_to(7);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().last_paid, 7u64);
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 450);
		roll_to(230);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_eq!(Crowdloan::accounts_payable(&1).unwrap().claimed_reward, 500);
		roll_to(330);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(1)),
			Error::<Test>::RewardsAlreadyClaimed
		);

		let expected = vec![
			crate::Event::InitialPaymentMade(1, 100),
			crate::Event::InitialPaymentMade(2, 100),
			crate::Event::RewardsPaid(1, 200),
			crate::Event::RewardsPaid(1, 50),
			crate::Event::RewardsPaid(1, 50),
			crate::Event::RewardsPaid(1, 50),
			crate::Event::RewardsPaid(1, 50),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn paying_late_joiner_works() {
	let pairs = get_ed25519_pairs(3);
	let signature: MultiSignature = pairs[0].sign(&3u64.encode()).into();
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		roll_to(12);
		assert_ok!(Crowdloan::associate_native_identity(
			Origin::signed(4),
			3,
			pairs[0].public().into(),
			signature.clone()
		));
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(3)));
		assert_eq!(Crowdloan::accounts_payable(&3).unwrap().last_paid, 12u64);
		assert_eq!(Crowdloan::accounts_payable(&3).unwrap().claimed_reward, 500);
		let expected = vec![
			crate::Event::InitialPaymentMade(1, 100),
			crate::Event::InitialPaymentMade(2, 100),
			crate::Event::InitialPaymentMade(3, 100),
			crate::Event::NativeIdentityAssociated(pairs[0].public().into(), 3, 500),
			crate::Event::RewardsPaid(3, 400),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn update_address_works() {
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));

		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(8)),
			Error::<Test>::NoAssociatedClaim
		);
		assert_ok!(Crowdloan::update_reward_address(Origin::signed(1), 8));
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().claimed_reward, 300);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(8)));
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&8).unwrap().claimed_reward, 400);
		// The initial payment is not
		let expected = vec![
			crate::Event::InitialPaymentMade(1, 100),
			crate::Event::InitialPaymentMade(2, 100),
			crate::Event::RewardsPaid(1, 200),
			crate::Event::RewardAddressUpdated(1, 8),
			crate::Event::RewardsPaid(8, 100),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn update_address_with_existing_address_works() {
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));

		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(1)));
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(2)));
		assert_ok!(Crowdloan::update_reward_address(Origin::signed(1), 2));
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().last_paid, 4u64);
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().claimed_reward, 600);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(1)),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(2)));
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().last_paid, 6u64);
		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().claimed_reward, 800);
		let expected = vec![
			crate::Event::InitialPaymentMade(1, 100),
			crate::Event::InitialPaymentMade(2, 100),
			crate::Event::RewardsPaid(1, 200),
			crate::Event::RewardsPaid(2, 200),
			crate::Event::RewardAddressUpdated(1, 2),
			crate::Event::RewardsPaid(2, 200),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn initialize_new_addresses() {
	empty().execute_with(|| {
		roll_to(2);
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(1), 500u32.into()),
				([2u8; 32].into(), Some(2), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		assert_eq!(Crowdloan::initialized(), true);

		roll_to(4);
		assert_noop!(
			Crowdloan::initialize_reward_vec(
				Origin::root(),
				vec![([1u8; 32].into(), Some(1), 500u32.into())],
				0,
				1
			),
			Error::<Test>::RewardVecAlreadyInitialized,
		);
	});
}

#[test]
fn initialize_new_addresses_with_batch() {
	empty().execute_with(|| {
		// This time should succeed trully
		roll_to(10);
		assert_ok!(mock::Call::Utility(UtilityCall::batch_all(vec![
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([4u8; 32].into(), Some(3), 1250)],
				0,
				2
			)),
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([5u8; 32].into(), Some(1), 1250)],
				1,
				2
			))
		]))
		.dispatch(Origin::root()));

		// Batch calls always succeed. We just need to check the inner event
		assert_ok!(
			mock::Call::Utility(UtilityCall::batch(vec![mock::Call::Crowdloan(
				crate::Call::initialize_reward_vec(vec![([4u8; 32].into(), Some(3), 500)], 0, 1)
			)]))
			.dispatch(Origin::root())
		);

		let expected = vec![
			pallet_utility::Event::BatchCompleted,
			pallet_utility::Event::BatchInterrupted(
				0,
				DispatchError::Module {
					index: 2,
					error: 8,
					message: None,
				},
			),
		];
		assert_eq!(batch_events(), expected);
	});
}

#[test]
fn first_free_claim_should_work() {
	empty().execute_with(|| {
		roll_to(2);
		assert_ok!(mock::Call::Utility(UtilityCall::batch_all(vec![
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([4u8; 32].into(), Some(1), 1250)],
				0,
				2
			)),
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([5u8; 32].into(), Some(2), 1250)],
				1,
				2
			))
		]))
		.dispatch(Origin::root()));

		assert_eq!(
			Crowdloan::accounts_payable(&2).unwrap().claimed_reward,
			250u128
		);

		// Block relay number is 2 post init initialization
		roll_to(4);

		assert_ok!(Crowdloan::my_first_claim(Origin::signed(2)));

		assert_eq!(Crowdloan::accounts_payable(&2).unwrap().last_paid, 4u64);

		assert_eq!(
			Crowdloan::accounts_payable(&2).unwrap().claimed_reward,
			500u128
		);

		// I cannot do this claim anymore
		assert_noop!(
			Crowdloan::my_first_claim(Origin::signed(2)),
			Error::<Test>::FirstClaimAlreadyDone
		);
	});
}
