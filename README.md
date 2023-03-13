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
    pub const Initialized: bool = false;
    pub const MinimumReward: Balance = 1000;
    pub const InitializationPayment: Perbill = Perbill::from_percent(25);
    pub const MaxInitContributorsSize: u32 = 500;
    pub const RewardAddressRelayVoteThreshold: Perbill = Perbill::from_percent(100);
    pub const SignatureNetworkIdentifier: &'static [u8] = b"chain-name";
}

impl pallet_crowdloan_rewards::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Initialized = Initialized;
    type InitializationPayment = InitializationPayment;
    type MaxInitContributors = MaxInitContributorsSize;
    type MinimumReward = MinimumReward;
    type RewardAddressRelayVoteThreshold = RewardAddressRelayVoteThreshold;
    type RewardCurrency = Balances;
    type RelayChainAccountId = sp_runtime::AccountId32;
    type RewardAddressChangeOrigin = EnsureSigned<AccountId>;
    type SignatureNetworkIdentifier = SignatureNetworkIdentifier;
    type RewardAddressAssociateOrigin = EnsureSigned<AccountId>;
    type VestingBlockNumber = cumulus_primitives_core::relay_chain::BlockNumber;
    type VestingBlockProvider = cumulus_pallet_parachain_system::RelaychainBlockNumberProvider<Self>;
    type WeightInfo = pallet_crowdloan_rewards::weights::SubstrateWeight<Runtime>;
}

construct_runtime! {
	// --snip--
	CrowdloanRewards: pallet_crowdloan_rewards
}
```


In your `chain_spec.rs` file:
```rust
const CROWDLOAN_FUND_POT: u128 = 1_000_000_000_000_000_000_000_000_u128; // Total reward amount
	
	
// Add crowdloan config in testnet_genesis
crowdloan_rewards: CrowdloanRewardsConfig {
	funded_amount: crowdloan_fund_pot,
},
```
