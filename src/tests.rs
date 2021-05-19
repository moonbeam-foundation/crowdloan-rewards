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
use account::{EthereumSignature, EthereumSigner};
use frame_support::dispatch::{DispatchError, Dispatchable};
use frame_support::{assert_noop, assert_ok};
use mock::*;
use parity_scale_codec::Encode;
use sha3::{Digest, Keccak256};
use sp_core::{ecdsa, Pair, H160};
use sp_runtime::traits::IdentifyAccount;
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
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// accounts_payable
		assert!(Crowdloan::accounts_payable(&H160::from([1u8; 20])).is_some());
		assert!(Crowdloan::accounts_payable(&H160::from([2u8; 20])).is_some());
		assert!(Crowdloan::accounts_payable(&H160::from([3u8; 20])).is_none());
		assert!(Crowdloan::accounts_payable(&H160::from([4u8; 20])).is_none());
		assert!(Crowdloan::accounts_payable(&H160::from([5u8; 20])).is_none());

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
	let signature: MultiSignature = pairs[0].sign(&H160::from([3u8; 20]).encode()).into();
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// 4 is not payable first
		assert!(Crowdloan::accounts_payable(&H160::from([3u8; 20])).is_none());
		roll_to(4);
		// Signature is wrong, prove fails
		assert_noop!(
			Crowdloan::associate_native_identity(
				Origin::signed(H160::from([4u8; 20])),
				H160::from([4u8; 20]),
				pairs[0].public().into(),
				signature.clone()
			),
			Error::<Test>::InvalidClaimSignature
		);
		// Signature is right, prove passes
		assert_ok!(Crowdloan::associate_native_identity(
			Origin::signed(H160::from([4u8; 20])),
			H160::from([3u8; 20]),
			pairs[0].public().into(),
			signature.clone()
		));
		// Signature is right, but address already claimed
		assert_noop!(
			Crowdloan::associate_native_identity(
				Origin::signed(H160::from([4u8; 20])),
				H160::from([3u8; 20]),
				pairs[0].public().into(),
				signature
			),
			Error::<Test>::AlreadyAssociated
		);
		// now three is payable
		assert!(Crowdloan::accounts_payable(&H160::from([3u8; 20])).is_some());
		assert!(Crowdloan::unassociated_contributions(pairs[0].public().as_array_ref()).is_none());
		assert!(Crowdloan::claimed_relay_chain_ids(pairs[0].public().as_array_ref()).is_some());

		let expected = vec![
			crate::Event::InitialPaymentMade(H160::from([1u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([2u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([3u8; 20]), 100),
			crate::Event::NativeIdentityAssociated(
				pairs[0].public().into(),
				H160::from([3u8; 20]),
				500,
			),
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
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		// 1 is payable
		assert!(Crowdloan::accounts_payable(&H160::from([1u8; 20])).is_some());
		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.last_paid,
			4u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.claimed_reward,
			300
		);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(H160::from([3u8; 20]))),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(5);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.last_paid,
			5u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.claimed_reward,
			350
		);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.last_paid,
			6u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.claimed_reward,
			400
		);
		roll_to(7);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.last_paid,
			7u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.claimed_reward,
			450
		);
		roll_to(230);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([1u8; 20]))
				.unwrap()
				.claimed_reward,
			500
		);
		roll_to(330);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(H160::from([1u8; 20]))),
			Error::<Test>::RewardsAlreadyClaimed
		);

		let expected = vec![
			crate::Event::InitialPaymentMade(H160::from([1u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([2u8; 20]), 100),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 200),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 50),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 50),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 50),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 50),
		];
		assert_eq!(events(), expected);
	});
}

#[test]
fn paying_late_joiner_works() {
	let pairs = get_ed25519_pairs(3);
	let signature: MultiSignature = pairs[0].sign(&H160::from([3u8; 20]).encode()).into();
	empty().execute_with(|| {
		// Insert contributors
		let pairs = get_ed25519_pairs(3);
		assert_ok!(Crowdloan::initialize_reward_vec(
			Origin::root(),
			vec![
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));
		roll_to(12);
		assert_ok!(Crowdloan::associate_native_identity(
			Origin::signed(H160::from([4u8; 20])),
			H160::from([3u8; 20]),
			pairs[0].public().into(),
			signature.clone()
		));
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[3u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([3u8; 20]))
				.unwrap()
				.last_paid,
			12u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([3u8; 20]))
				.unwrap()
				.claimed_reward,
			500
		);
		let expected = vec![
			crate::Event::InitialPaymentMade(H160::from([1u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([2u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([3u8; 20]), 100),
			crate::Event::NativeIdentityAssociated(
				pairs[0].public().into(),
				H160::from([3u8; 20]),
				500,
			),
			crate::Event::RewardsPaid(H160::from([3u8; 20]), 400),
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
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));

		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(H160::from([8u8; 20]))),
			Error::<Test>::NoAssociatedClaim
		);
		assert_ok!(Crowdloan::update_reward_address(
			Origin::signed(H160::from([1u8; 20])),
			H160::from([8u8; 20])
		));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([8u8; 20]))
				.unwrap()
				.last_paid,
			4u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([8u8; 20]))
				.unwrap()
				.claimed_reward,
			300
		);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[8u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([8u8; 20]))
				.unwrap()
				.last_paid,
			6u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([8u8; 20]))
				.unwrap()
				.claimed_reward,
			400
		);
		// The initial payment is not
		let expected = vec![
			crate::Event::InitialPaymentMade(H160::from([1u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([2u8; 20]), 100),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 200),
			crate::Event::RewardAddressUpdated(H160::from([1u8; 20]), H160::from([8u8; 20])),
			crate::Event::RewardsPaid(H160::from([8u8; 20]), 100),
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
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
				(pairs[0].public().into(), None, 500u32.into()),
				(pairs[1].public().into(), None, 500u32.into()),
				(pairs[2].public().into(), None, 500u32.into())
			],
			0,
			5
		));

		roll_to(4);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[1u8; 20]
		))));
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[2u8; 20]
		))));
		assert_ok!(Crowdloan::update_reward_address(
			Origin::signed(H160::from([1u8; 20])),
			H160::from([2u8; 20])
		));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([2u8; 20]))
				.unwrap()
				.last_paid,
			4u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([2u8; 20]))
				.unwrap()
				.claimed_reward,
			600
		);
		assert_noop!(
			Crowdloan::show_me_the_money(Origin::signed(H160::from([1u8; 20]))),
			Error::<Test>::NoAssociatedClaim
		);
		roll_to(6);
		assert_ok!(Crowdloan::show_me_the_money(Origin::signed(H160::from(
			[2u8; 20]
		))));
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([2u8; 20]))
				.unwrap()
				.last_paid,
			6u64
		);
		assert_eq!(
			Crowdloan::accounts_payable(&H160::from([2u8; 20]))
				.unwrap()
				.claimed_reward,
			800
		);
		let expected = vec![
			crate::Event::InitialPaymentMade(H160::from([1u8; 20]), 100),
			crate::Event::InitialPaymentMade(H160::from([2u8; 20]), 100),
			crate::Event::RewardsPaid(H160::from([1u8; 20]), 200),
			crate::Event::RewardsPaid(H160::from([2u8; 20]), 200),
			crate::Event::RewardAddressUpdated(H160::from([1u8; 20]), H160::from([2u8; 20])),
			crate::Event::RewardsPaid(H160::from([2u8; 20]), 200),
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
				([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into()),
				([2u8; 32].into(), Some(H160::from([2u8; 20])), 500u32.into()),
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
				vec![([1u8; 32].into(), Some(H160::from([1u8; 20])), 500u32.into())],
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
				vec![([4u8; 32].into(), Some(H160::from([3u8; 20])), 1250)],
				0,
				2
			)),
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([5u8; 32].into(), Some(H160::from([1u8; 20])), 1250)],
				1,
				2
			))
		]))
		.dispatch(Origin::root()));

		// Batch calls always succeed. We just need to check the inner event
		assert_ok!(
			mock::Call::Utility(UtilityCall::batch(vec![mock::Call::Crowdloan(
				crate::Call::initialize_reward_vec(
					vec![([4u8; 32].into(), Some(H160::from([3u8; 20])), 500)],
					0,
					1
				)
			)]))
			.dispatch(Origin::root())
		);

		let expected = vec![
			pallet_utility::Event::BatchCompleted,
			pallet_utility::Event::BatchInterrupted(
				0,
				DispatchError::Module {
					index: 2,
					error: 7,
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
		let secret_key = [1u8; 32];
		let secret = secp256k1::SecretKey::parse_slice(&secret_key).unwrap();
		let pair = ecdsa::Pair::from_seed_slice(&secret_key).unwrap();

		let account: EthereumSigner = pair.public().into();

		let data = account.using_encoded(to_ascii_hex);

		let mut m = [0u8; 32];
		m.copy_from_slice(Keccak256::digest(&data).as_slice());

		let message = secp256k1::Message::parse(&m);
		let signature: ecdsa::Signature = secp256k1::sign(&message, &secret).into();

		let new_signature: EthereumSignature = signature.into();

		roll_to(2);
		assert_ok!(mock::Call::Utility(UtilityCall::batch_all(vec![
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([4u8; 32].into(), Some(account.clone().into_account()), 1250)],
				0,
				2
			)),
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([5u8; 32].into(), Some(H160::from([1u8; 20])), 1250)],
				1,
				2
			))
		]))
		.dispatch(Origin::root()));

		assert_eq!(
			Crowdloan::accounts_payable(&account.clone().into_account())
				.unwrap()
				.claimed_reward,
			250u128
		);

		// Block relay number is 2 post init initialization
		roll_to(4);

		assert_ok!(Crowdloan::my_first_claim(
			Origin::none(),
			account.clone().into_account(),
			new_signature.clone().into()
		));

		assert_eq!(
			Crowdloan::accounts_payable(&account.clone().into_account())
				.unwrap()
				.last_paid,
			4u64
		);

		assert_eq!(
			Crowdloan::accounts_payable(&account.clone().into_account())
				.unwrap()
				.claimed_reward,
			500u128
		);

		// I cannot do this claim anymore
		assert_noop!(
			Crowdloan::my_first_claim(
				Origin::none(),
				account.clone().into_account(),
				new_signature.into()
			),
			Error::<Test>::FirstClaimAlreadyDone
		);
	});
}

#[test]
fn free_claim_with_invalid_signature_does_not_work() {
	empty().execute_with(|| {
		let secret_key = [1u8; 32];
		let secret_key2 = [2u8; 32];
		let secret2 = secp256k1::SecretKey::parse_slice(&secret_key2).unwrap();
		let pair = ecdsa::Pair::from_seed_slice(&secret_key).unwrap();
		let pair2 = ecdsa::Pair::from_seed_slice(&secret_key2).unwrap();

		let account: EthereumSigner = pair.public().into();
		let account2: EthereumSigner = pair2.public().into();
		let data = account.using_encoded(to_ascii_hex);
		let data2 = account2.using_encoded(to_ascii_hex);

		// We create a fake signature, account 2 signing account 1 payload
		let mut m = [0u8; 32];
		m.copy_from_slice(Keccak256::digest(&data).as_slice());

		let message = secp256k1::Message::parse(&m);
		let signature: ecdsa::Signature = secp256k1::sign(&message, &secret2).into();

		let fake_sig: EthereumSignature = signature.into();

		// We create a valid payload, accounnt 2 signing account 2
		let mut m = [0u8; 32];
		m.copy_from_slice(Keccak256::digest(&data2).as_slice());

		let message2 = secp256k1::Message::parse(&m);
		let signature: ecdsa::Signature = secp256k1::sign(&message2, &secret2).into();

		let signature2: EthereumSignature = signature.into();

		roll_to(2);
		assert_ok!(mock::Call::Utility(UtilityCall::batch_all(vec![
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([4u8; 32].into(), Some(account.clone().into_account()), 1250)],
				0,
				2
			)),
			mock::Call::Crowdloan(crate::Call::initialize_reward_vec(
				vec![([5u8; 32].into(), Some(H160::from([1u8; 20])), 1250)],
				1,
				2
			))
		]))
		.dispatch(Origin::root()));

		// We made a first payment

		assert_eq!(
			Crowdloan::accounts_payable(&account.clone().into_account())
				.unwrap()
				.claimed_reward,
			250u128
		);

		roll_to(4);

		// Here the siganture is done signing account 1 instead of 2. Wrong sig error
		assert_noop!(
			Crowdloan::my_first_claim(
				Origin::none(),
				account2.clone().into_account(),
				fake_sig.into()
			),
			Error::<Test>::InvalidFreeClaimSignature
		);

		// Correct signature but no associated claim
		assert_noop!(
			Crowdloan::my_first_claim(
				Origin::none(),
				account2.clone().into_account(),
				signature2.into()
			),
			Error::<Test>::NoAssociatedClaim
		);
	});
}
