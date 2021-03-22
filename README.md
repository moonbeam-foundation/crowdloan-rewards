# Crowdloan Rewards Pallet

Distribute rewards for crowdloan participation in parachain-native tokens.

## Context
Polkadot and Kusama will allocate parachain slots using an [auction mechanism]
(https://wiki.polkadot.network/docs/en/learn-auction). Bidders can be normal relay chain accounts,
or [crowdloans](https://wiki.polkadot.network/docs/en/learn-crowdloans). A parachain project may use
a crowdloan to allow its community to pool funds to bid for a slot. Pooled funds will be locked up,
so parachain projects will need to reward their community for taking the opportunity cost of locking
tokens in a crowdfund.

## Design Overview

There are good docs in the crate. For now see them in `src/lib.rs`.

## Using this pallet in your parachain runtime

First you will need to make sure your project is using the same Substrate dependencies as this
pallet.

In your `Cargo.toml` file:
```toml
[dependencies]
# --snip--
pallet-crowdloan-rewards = { git = "https://github.com/purestake/crowdloan-rewards", default-features = false, branch = "main" }

[features]
default = ['std']
std = [
  # --snip--
  'pallet-crowdloan-rewards/std',
]
```

In your `lib.rs` file:
```rust
parameter_types! {
	pub const VestingPeriod: BlockNumber = 1000;
}

impl pallet_crowdloan_rewards::Config for Runtime {
	type Event = Event;
	type RelayChainAccountId = sp_runtime::AccountId32;
	type RewardCurrency = Balances;
	type VestingPeriod = VestingPeriod;
}

construct_runtime! {
	// --snip--
	CrowdloanRewards: pallet_crowdloan_rewards::{Module, Call, Storage, Config<T>, Event<T>}
}
```
