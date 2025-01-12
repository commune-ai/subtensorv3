#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]
// Edit this file to define custom logic or remove it if it is not needed.
// Learn more about FRAME and the core library of Substrate FRAME pallets:
// <https://docs.substrate.io/reference/frame-pallets/>
pub use pallet::*;

use frame_system::{
	self as system,
	ensure_signed
};

use frame_support::{
	dispatch,
	dispatch::{
		DispatchInfo,
		PostDispatchInfo
	}, ensure, 
	traits::{
		Currency, 
		ExistenceRequirement,
		tokens::{
			WithdrawReasons
		},
		IsSubType,
		}
};

use sp_std::marker::PhantomData;
use codec::{Decode, Encode};
use sp_runtime::{
	traits::{
		Dispatchable,
		DispatchInfoOf,
		SignedExtension,
		PostDispatchInfoOf
	},
	transaction_validity::{
		TransactionValidity,
		TransactionValidityError
	}
};
use scale_info::TypeInfo;
use frame_support::sp_runtime::transaction_validity::ValidTransaction;

// ============================
//	==== Benchmark Imports =====
// ============================
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks;

// =========================
//	==== Pallet Imports =====
// =========================
mod block_step;

mod epoch;
mod math;
mod network;
mod registration;
mod serving;
mod staking;
mod utils;
mod uids;
mod weights;
pub mod module;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use frame_support::traits::Currency;
	use frame_support::sp_std::vec;
	use serde::{Serialize, Deserialize};
	use serde_with::{serde_as, DisplayFromStr};
	use frame_support::inherent::Vec;
	use scale_info::prelude::string::String;


	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		// --- Currency type that will be used to place deposits on modules
		type Currency: Currency<Self::AccountId> + Send + Sync;

		// =================================
		// ==== Initial Value Constants ====
		// =================================
		#[pallet::constant] // Initial currency issuance.
		type InitialIssuance: Get<u64>;
		#[pallet::constant] // Initial min allowed weights setting.
		type InitialMinAllowedWeights: Get<u16>;
		#[pallet::constant] // Initial Emission Ratio
		type InitialEmissionValue: Get<u16>;
		#[pallet::constant] // Initial max weight limit.
		type InitialMaxWeightsLimit: Get<u16>;
		#[pallet::constant] // Tempo for each network
		type InitialTempo: Get<u16>;
		#[pallet::constant] // Initial adjustment interval.
		type InitialAdjustmentInterval: Get<u16>;
		#[pallet::constant] // Initial bonds moving average.
		type InitialBondsMovingAverage: Get<u64>;
		#[pallet::constant] // Initial target registrations per interval.
		type InitialTargetRegistrationsPerInterval: Get<u16>;
		#[pallet::constant] // Max UID constant.
		type InitialMaxAllowedUids: Get<u16>;
		#[pallet::constant] // Immunity Period Constant.
		type InitialImmunityPeriod: Get<u16>;
		#[pallet::constant] // Activity constant
		type InitialActivityCutoff: Get<u16>;
		#[pallet::constant] // Initial max registrations per block.
		type InitialMaxRegistrationsPerBlock: Get<u16>;
		#[pallet::constant] // Initial pruning score for each module
		type InitialPruningScore: Get<u16>;	
		#[pallet::constant] // Initial serving rate limit.
		type InitialServingRateLimit: Get<u64>;
		#[pallet::constant] // Initial transaction rate limit.
		type InitialTxRateLimit: Get<u64>;
	}

	pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

	// ============================
	// ==== Staking + Accounts ====
	// ============================
	#[pallet::type_value] 
	pub fn DefaultAccountTake<T: Config>() -> u64 { 0 }
	#[pallet::type_value]
	pub fn DefaultBlockEmission<T: Config>() -> u64 {1_000_000_000}
	#[pallet::type_value] 
	pub fn DefaultTotalIssuance<T: Config>() -> u64 { T::InitialIssuance::get() }
	#[pallet::type_value] 
	pub fn DefaultAccount<T: Config>() -> T::AccountId { T::AccountId::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap()}

	#[pallet::storage] // --- ITEM ( total_stake )
	pub type TotalStake<T> = StorageValue<_, u64, ValueQuery>;
	#[pallet::storage] // --- ITEM ( default_take )
	pub type DefaultTake<T> = StorageValue<_, u16, ValueQuery, DefaultDefaultTake<T>>;
	#[pallet::storage] // --- ITEM ( global_block_emission )
	pub type BlockEmission<T> = StorageValue<_, u64, ValueQuery, DefaultBlockEmission<T>>;
	#[pallet::storage] // --- ITEM ( total_issuance )
	pub type TotalIssuance<T> = StorageValue<_, u64, ValueQuery, DefaultTotalIssuance<T>>;
	#[pallet::storage] // --- MAP ( hot ) --> stake | Returns the total amount of stake under a key.
    pub type TotalKeyStake<T:Config> = StorageMap<_, Identity, T::AccountId, u64, ValueQuery, DefaultAccountTake<T>>;

	#[pallet::storage] // --- DMAP ( key ) --> stake | Returns the stake under a key prefixed by key.
    pub type Stake<T:Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u64, ValueQuery, DefaultDefaultTake<T>>;

	#[pallet::type_value] 
	pub fn DefaultLastAdjustmentBlock<T: Config>() -> u64 { 0 }
	#[pallet::type_value]
	pub fn DefaultRegistrationsThisBlock<T: Config>() ->  u16 { 0}
	#[pallet::type_value] 
	pub fn DefaultMaxRegistrationsPerBlock<T: Config>() -> u16 { T::InitialMaxRegistrationsPerBlock::get() }

	#[pallet::storage] // --- MAP ( netuid ) -->  Block at last adjustment.
	pub type LastAdjustmentBlock<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultLastAdjustmentBlock<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> Registration this Block.
	pub type RegistrationsThisBlock<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultRegistrationsThisBlock<T>>;
	#[pallet::storage] // --- ITEM( global_max_registrations_per_block ) 
	pub type MaxRegistrationsPerBlock<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultMaxRegistrationsPerBlock<T> >;

	// ==============================
	// ==== Networkworks Storage =====
	// ==============================
	#[pallet::type_value] 
	pub fn DefaultN<T:Config>() -> u16 { 0 }
	#[pallet::type_value]
	pub fn DefaultNeworksAdded<T: Config>() ->  bool { false }
	#[pallet::type_value]
	pub fn DefaultIsNetworkMember<T: Config>() ->  bool { false }


	#[pallet::storage] // --- ITEM( tota_number_of_existing_networks )
	pub type TotalNetworks<T> = StorageValue<_, u16, ValueQuery>;
	#[pallet::storage] // --- MAP ( netuid ) --> network_n (Number of UIDs in the network).
	pub type NetworkworkN<T:Config> = StorageMap< _, Identity, u16, u16, ValueQuery, DefaultN<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> network_is_added
	pub type NetworksAdded<T:Config> = StorageMap<_, Identity, u16, bool, ValueQuery, DefaultNeworksAdded<T>>;	
	#[pallet::storage] // --- DMAP ( netuid, netuid ) -> registration_requirement
	pub type NetworkConnect<T:Config> = StorageDoubleMap<_, Identity, u16, Identity, u16, u16, OptionQuery>;
	#[pallet::storage] // --- DMAP ( key, netuid ) --> bool
	pub type IsNetworkMember<T:Config> = StorageDoubleMap<_, Blake2_128Concat, T::AccountId, Identity, u16, bool, ValueQuery, DefaultIsNetworkMember<T>>;

	// ==============================
	// ==== Networkwork Features =====
	// ==============================
	#[pallet::type_value]
	pub fn DefaultEmissionValues<T: Config>() ->  u64 { 0 }
	#[pallet::type_value]
	pub fn DefaultPendingEmission<T: Config>() ->  u64 { 0 }
	#[pallet::type_value] 
	pub fn DefaultBlocksSinceLastStep<T: Config>() -> u64 { 0 }
	#[pallet::type_value] 
	pub fn DefaultLastMechansimStepBlock<T: Config>() -> u64 { 0 }
	#[pallet::type_value]
	pub fn DefaultTempo<T: Config>() -> u16 { T::InitialTempo::get() }

	#[pallet::storage] // --- MAP ( netuid ) --> tempo
	pub type Tempo<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultTempo<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> emission_values
	pub type EmissionValues<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultEmissionValues<T>>;
	#[pallet::storage] // --- MAP ( netuid ) --> pending_emission
	pub type PendingEmission<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultPendingEmission<T>>;
	#[pallet::storage] // --- MAP ( netuid ) --> blocks_since_last_step.
	pub type BlocksSinceLastStep<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultBlocksSinceLastStep<T>>;
	#[pallet::storage] // --- MAP ( netuid ) --> last_mechanism_step_block
	pub type LastMechansimStepBlock<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultLastMechansimStepBlock<T> >;

	// =================================
	// ==== Module / Promo Endpoints =====
	// =================================
	

	#[derive(Decode, Encode, PartialEq, Eq, Clone, Debug)]
	pub struct Module<T: Config> {
		pub key: T::AccountId,
		pub uid: Compact<u16>,
		pub name : Vec<u8>, // --- Module name.
		pub netuid: Compact<u16>,
        pub ip: u128, // --- Module u128 encoded ip address of type v6 or v4.
        pub port: u16, // --- Module u16 encoded port.
		pub uri : Vec<u8>, // --- Module uri.
		pub last_update: Compact<u64>,
	}

	#[pallet::storage] // --- MAP ( netuid, key ) --> module
	pub(super) type Modules<T:Config> = StorageDoubleMap<_, Identity, u16, Blake2_128Concat, T::AccountId, Module, OptionQuery>;

	// Rate limiting
	#[pallet::type_value]
	pub fn DefaultTxRateLimit<T: Config>() -> u64 { T::InitialTxRateLimit::get() }
	#[pallet::type_value]
	pub fn DefaultLastTxBlock<T: Config>() -> u64 { 0 }

	#[pallet::storage] // --- ITEM ( tx_rate_limit )
	pub(super) type TxRateLimit<T> = StorageValue<_, u64, ValueQuery, DefaultTxRateLimit<T>>;
	#[pallet::storage] // --- MAP ( key ) --> last_block
	pub(super) type LastTxBlock<T:Config> = StorageMap<_, Identity, T::AccountId, u64, ValueQuery, DefaultLastTxBlock<T>>;


	#[pallet::type_value] 
	pub fn DefaultServingRateLimit<T: Config>() -> u64 { T::InitialServingRateLimit::get() }

	#[pallet::storage] // --- MAP ( netuid ) --> serving_rate_limit
	pub type ServingRateLimit<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultServingRateLimit<T>> ;

	// =======================================
	// ==== Networkwork Hyperparam storage ====
	// =======================================	
	#[pallet::type_value] 
	pub fn DefaultWeightsSetRateLimit<T: Config>() -> u64 { 0 }
	#[pallet::type_value] 
	pub fn DefaultBlockAtRegistration<T: Config>() -> u64 { 0 }
	#[pallet::type_value] 
	pub fn DefaultMaxAllowedUids<T: Config>() -> u16 { T::InitialMaxAllowedUids::get() }
	#[pallet::type_value] 
	pub fn DefaultImmunityPeriod<T: Config>() -> u16 { T::InitialImmunityPeriod::get() }
	#[pallet::type_value] 
	pub fn DefaultActivityCutoff<T: Config>() -> u16 { T::InitialActivityCutoff::get() }
	#[pallet::type_value] 
	pub fn DefaultMaxWeightsLimit<T: Config>() -> u16 { T::InitialMaxWeightsLimit::get() }
	#[pallet::type_value] 
	pub fn DefaultMinAllowedWeights<T: Config>() -> u16 { T::InitialMinAllowedWeights::get() }
	#[pallet::type_value]
	pub fn DefaultAdjustmentInterval<T: Config>() -> u16 { T::InitialAdjustmentInterval::get() }
	#[pallet::type_value] 
	pub fn DefaultTargetRegistrationsPerInterval<T: Config>() -> u16 { T::InitialTargetRegistrationsPerInterval::get() }

	#[pallet::storage] // --- MAP ( netuid ) --> uid, we use to record uids to prune at next epoch.
    pub type ModulesToPruneAtNextEpoch<T:Config> = StorageMap<_, Identity, u16, u16, ValueQuery>;
	#[pallet::storage] // --- MAP ( netuid ) --> registrations_this_interval
	pub type RegistrationsThisInterval<T:Config> = StorageMap<_, Identity, u16, u16, ValueQuery>;
	#[pallet::storage] // --- MAP ( netuid ) --> max_allowed_uids
	pub type MaxAllowedUids<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultMaxAllowedUids<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> immunity_period
	pub type ImmunityPeriod<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultImmunityPeriod<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> activity_cutoff
	pub type ActivityCutoff<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultActivityCutoff<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> max_weight_limit
	pub type MaxWeightsLimit<T> = StorageMap< _, Identity, u16, u16, ValueQuery, DefaultMaxWeightsLimit<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> min_allowed_weights
	pub type MinAllowedWeights<T> = StorageMap< _, Identity, u16, u16, ValueQuery, DefaultMinAllowedWeights<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> adjustment_interval
	pub type AdjustmentInterval<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultAdjustmentInterval<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> weights_set_rate_limit
	pub type WeightsSetRateLimit<T> = StorageMap<_, Identity, u16, u64, ValueQuery, DefaultWeightsSetRateLimit<T> >;
	#[pallet::storage] // --- MAP ( netuid ) --> target_registrations_this_interval
	pub type TargetRegistrationsPerInterval<T> = StorageMap<_, Identity, u16, u16, ValueQuery, DefaultTargetRegistrationsPerInterval<T> >;
	#[pallet::storage] // --- DMAP ( netuid, uid ) --> block_at_registration
	pub type BlockAtRegistration<T:Config> = StorageDoubleMap<_, Identity, u16, Identity, u16, u64, ValueQuery, DefaultBlockAtRegistration<T> >;

	// =======================================
	// ==== Networkwork Consensus Storage  ====
	// =======================================
	#[pallet::type_value] 
	pub fn EmptyU16Vec<T:Config>() -> Vec<u16> { vec![] }
	#[pallet::type_value] 
	pub fn EmptyU64Vec<T:Config>() -> Vec<u64> { vec![] }
	#[pallet::type_value] 
	pub fn EmptyBoolVec<T:Config>() -> Vec<bool> { vec![] }
	#[pallet::type_value] 
	pub fn DefaultBonds<T:Config>() -> Vec<(u16, u16)> { vec![] }
	#[pallet::type_value] 
	pub fn DefaultWeights<T:Config>() -> Vec<(u16, u16)> { vec![] }
	#[pallet::type_value] 
	pub fn DefaultKey<T:Config>() -> T::AccountId { T::AccountId::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap() }

	#[pallet::storage] // --- DMAP ( netuid ) --> emission
	pub(super) type LoadedEmission<T:Config> = StorageMap< _, Identity, u16, Vec<(T::AccountId, u64)>, OptionQuery >;

	#[pallet::storage] // --- DMAP ( netuid ) --> active
	pub(super) type Active<T:Config> = StorageMap< _, Identity, u16, Vec<bool>, ValueQuery, EmptyBoolVec<T> >;
	#[pallet::storage] // --- DMAP ( netuid ) --> rank
	pub(super) type Rank<T:Config> = StorageMap< _, Identity, u16, Vec<u16>, ValueQuery, EmptyU16Vec<T>>;
	#[pallet::storage] // --- DMAP ( netuid ) --> incentive
	pub(super) type Incentive<T:Config> = StorageMap< _, Identity, u16, Vec<u16>, ValueQuery, EmptyU16Vec<T>>;
	#[pallet::storage] // --- DMAP ( netuid ) --> dividends
	pub(super) type Dividends<T:Config> = StorageMap< _, Identity, u16, Vec<u16>, ValueQuery, EmptyU16Vec<T>>;
	#[pallet::storage] // --- DMAP ( netuid ) --> dividends
	pub(super) type Emission<T:Config> = StorageMap< _, Identity, u16, Vec<u64>, ValueQuery, EmptyU64Vec<T>>;
	#[pallet::storage] // --- DMAP ( netuid ) --> last_update
	pub(super) type LastUpdate<T:Config> = StorageMap< _, Identity, u16, Vec<u64>, ValueQuery, EmptyU64Vec<T>>;

	#[pallet::storage] // --- DMAP ( netuid ) --> pruning_scores
	pub(super) type PruningScores<T:Config> = StorageMap< _, Identity, u16, Vec<u16>, ValueQuery, EmptyU16Vec<T> >;


	#[pallet::storage] // --- DMAP ( netuid, key ) --> uid
	pub(super) type Uids<T:Config> = StorageDoubleMap<_, Identity, u16, Blake2_128Concat, T::AccountId, u16, OptionQuery>;
	#[pallet::storage] // --- DMAP ( netuid, uid ) --> key
	pub(super) type Keys<T:Config> = StorageDoubleMap<_, Identity, u16, Identity, u16, T::AccountId, ValueQuery, DefaultKey<T> >;
	#[pallet::storage] // --- DMAP ( netuid, uid ) --> weights
    pub(super) type Weights<T:Config> = StorageDoubleMap<_, Identity, u16, Identity, u16, Vec<(u16, u16)>, ValueQuery, DefaultWeights<T> >;
	#[pallet::storage] // --- DMAP ( netuid, uid ) --> bonds
    pub(super) type Bonds<T:Config> = StorageDoubleMap<_, Identity, u16, Identity, u16, Vec<(u16, u16)>, ValueQuery, DefaultBonds<T> >;

	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/main-docs/build/events-errors/
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// Event documentation should end with an array that provides descriptive names for event
		// parameters. [something, who]
		NetworkAdded( u16, u16 ),	// --- Event created when a new network is added.
		NetworkRemoved( u16 ), // --- Event created when a network is removed.
		StakeAdded( T::AccountId, u64 ), // --- Event created when stake has been transfered from the a key account onto the key staking account.
		StakeRemoved( T::AccountId, u64 ), // --- Event created when stake has been removed from the key staking account onto the key account.
		WeightsSet( u16, u16 ), // ---- Event created when a caller successfully set's their weights on a network.
		ModuleRegistered( u16, u16, T::AccountId ), // --- Event created when a new module account has been registered to the chain.
		BulkModulesRegistered( u16, u16 ), // --- Event created when multiple uids have been concurrently registered.
		BulkBalancesSet(u16, u16),
		MaxAllowedUidsSet( u16, u16 ), // --- Event created when max allowed uids has been set for a networkwor.
		MaxWeightLimitSet( u16, u16 ), // --- Event created when the max weight limit has been set.
		AdjustmentIntervalSet( u16, u16 ), // --- Event created when the adjustment interval is set for a network.
		RegistrationPerIntervalSet( u16, u16 ), // --- Event created when registeration per interval is set for a network.
		MaxRegistrationsPerBlockSet( u16, u16), // --- Event created when we set max registrations per block
		ActivityCutoffSet( u16, u16 ), // --- Event created when an activity cutoff is set for a network.
		MinAllowedWeightSet( u16, u16 ), // --- Event created when minimun allowed weight is set for a network.
		WeightsSetRateLimitSet( u16, u64 ), // --- Event create when weights set rate limit has been set for a network.
		ImmunityPeriodSet( u16, u16), // --- Event created when immunity period is set for a network.
		ModuleServed( u16, T::AccountId ), // --- Event created when the module server information is added to the network.
		PrometheusServed( u16, T::AccountId ), // --- Event created when the module server information is added to the network.
		EmissionValuesSet(), // --- Event created when emission ratios fr all networks is set.
		ServingRateLimitSet( u16, u64 ), // --- Event created when setting the prometheus serving rate limit.
		TxRateLimitSet( u64 ), // --- Event created when setting the transaction rate limit.
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		NetworkDoesNotExist, // --- Thrown when the network does not exist.
		NetworkExist, // --- Thrown when the network already exist.
		InvalidIpType, // ---- Thrown when the user tries to serve an module which is not of type	4 (IPv4) or 6 (IPv6).
		InvalidIpAddress, // --- Thrown when an invalid IP address is passed to the serve function.
		NotRegistered, // ---- Thrown when the caller requests setting or removing data from a module which does not exist in the active set.
		NotEnoughStaketoWithdraw, // ---- Thrown when the caller requests removing more stake then there exists in the staking account. See: fn remove_stake.
		NotEnoughBalanceToStake, //  ---- Thrown when the caller requests adding more stake than there exists in the cold key account. See: fn add_stake
		BalanceWithdrawalError, // ---- Thrown when the caller tries to add stake, but for some reason the requested amount could not be withdrawn from the key account
		WeightVecNotEqualSize, // ---- Thrown when the caller attempts to set the weight keys and values but these vectors have different size.
		DuplicateUids, // ---- Thrown when the caller attempts to set weights with duplicate uids in the weight matrix.
		InvalidUid, // ---- Thrown when a caller attempts to set weight to at least one uid that does not exist in the metagraph.
		NotSettingEnoughWeights, // ---- Thrown when the dispatch attempts to set weights on chain with fewer elements than are allowed.
		TooManyRegistrationsThisBlock, // ---- Thrown when registrations this block exceeds allowed number.
		AlreadyRegistered, // ---- Thrown when the caller requests registering a module which already exists in the active set.
		MaxAllowedUIdsNotAllowed, // ---  Thrown if the vaule is invalid for MaxAllowedUids
		CouldNotConvertToBalance, // ---- Thrown when the dispatch attempts to convert between a u64 and T::balance but the call fails.
		StakeAlreadyAdded, // --- Thrown when the caller requests adding stake for a key to the total stake which already added
		MaxWeightExceeded, // --- Thrown when the dispatch attempts to set weights on chain with where any normalized weight is more than MaxWeightLimit.
		StorageValueOutOfRange, // --- Thrown when the caller attempts to set a storage value outside of its allowed range.
		TempoHasNotSet, // --- Thrown when tempo has not set
		InvalidTempo, // --- Thrown when tempo is not valid
		EmissionValuesDoesNotMatchNetworks, // --- Thrown when number or recieved emission rates does not match number of networks
		InvalidEmissionValues, // --- Thrown when emission ratios are not valid (did not sum up to 10^9)
		DidNotPassConnectedNetworkRequirement, // --- Thrown when a key attempts to register into a network without passing the  registration requirment from another network.
		SettingWeightsTooFast, // --- Thrown if the key attempts to set weights twice withing net_tempo/2 blocks.
		IncorrectNetworkVersionKey, // --- Thrown of a validator attempts to set weights from a validator with incorrect code base key.
		ServingRateLimitExceeded, // --- Thrown when an module or prometheus serving exceeds the rate limit for a registered module.
		BalanceSetError, // --- Thrown when an error occurs setting a balance
		MaxAllowedUidsExceeded, // --- Thrown when number of accounts going to be registered exceed MaxAllowedUids for the network.
		TooManyUids, // ---- Thrown when the caller attempts to set weights with more uids than allowed.
		TxRateLimitExceeded, // --- Thrown when a transactor exceeds the rate limit for transactions.
		RegistrationDisabled // --- Thrown when registration is disabled
	}

	// ==================
	// ==== Genesis =====
	// ==================

	#[pallet::genesis_config]
	#[cfg(feature = "std")]
	pub struct GenesisConfig<T: Config> {
		pub stakes: Vec<(T::AccountId, Vec<(T::AccountId, (u64, u16))>)>,
		pub balances_issuance: u64
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { 
				stakes: Default::default(),
				balances_issuance: 0
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			// Set initial total issuance from balances
			TotalIssuance::<T>::put(self.balances_issuance);

			// Network config values
			let netname: Vec<u8> = "commune".as_bytes().to_vec();
			let netuid: u16 = 0;
			let tempo = 99;
			let max_uids = 4096;
			
			// The functions for initializing new networks/setting defaults cannot be run directly from genesis functions like extrinsics would
			// --- Set this network uid to alive.
			NetworksAdded::<T>::insert(netuid, true);
			
			// --- Fill tempo memory item.
			Tempo::<T>::insert(netuid, tempo);
	

			// Make network parameters explicit.
			if !Tempo::<T>::contains_key( netuid ) { Tempo::<T>::insert( netuid, Tempo::<T>::get( netuid ));}
			if !MaxAllowedUids::<T>::contains_key( netuid ) { MaxAllowedUids::<T>::insert( netuid, MaxAllowedUids::<T>::get( netuid ));}
			if !ImmunityPeriod::<T>::contains_key( netuid ) { ImmunityPeriod::<T>::insert( netuid, ImmunityPeriod::<T>::get( netuid ));}
			if !ActivityCutoff::<T>::contains_key( netuid ) { ActivityCutoff::<T>::insert( netuid, ActivityCutoff::<T>::get( netuid ));}
			if !EmissionValues::<T>::contains_key( netuid ) { EmissionValues::<T>::insert( netuid, EmissionValues::<T>::get( netuid ));}   
			if !MaxWeightsLimit::<T>::contains_key( netuid ) { MaxWeightsLimit::<T>::insert( netuid, MaxWeightsLimit::<T>::get( netuid ));}
			if !MinAllowedWeights::<T>::contains_key( netuid ) { MinAllowedWeights::<T>::insert( netuid, MinAllowedWeights::<T>::get( netuid )); }
			if !RegistrationsThisInterval::<T>::contains_key( netuid ) { RegistrationsThisInterval::<T>::insert( netuid, RegistrationsThisInterval::<T>::get( netuid ));}

			// Set max allowed uids
			MaxAllowedUids::<T>::insert(netuid, max_uids);

			let mut next_uid = 0;

			for (key) in self.stakes.iter() {

				let (stake, uid) = stake_uid;

				// Expand Yuma Consensus with new position.
				Rank::<T>::mutate(netuid, |v| v.push(0));
				Active::<T>::mutate(netuid, |v| v.push(true));
				Emission::<T>::mutate(netuid, |v| v.push(0));
				Incentive::<T>::mutate(netuid, |v| v.push(0));
				Dividends::<T>::mutate(netuid, |v| v.push(0));
				LastUpdate::<T>::mutate(netuid, |v| v.push(0));
				PruningScores::<T>::mutate(netuid, |v| v.push(0));
		
				// Insert account information.
				Keys::<T>::insert(netuid, uid, key.clone()); // Make key - uid association.
				Uids::<T>::insert(netuid, key.clone(), uid); // Make uid - key association.
				BlockAtRegistration::<T>::insert(netuid, uid, 0); // Fill block at registration.
				IsNetworkMember::<T>::insert(key.clone(), netuid, true); // Fill network is member.
	
				TotalKeyStake::<T>::insert(key.clone(), stake);

				// Update total issuance value
				TotalIssuance::<T>::put(TotalIssuance::<T>::get().saturating_add(*stake));

				Stake::<T>::insert(key.clone(), stake);

				next_uid += 1;
				}
			}

	 	 	// Set correct length for Network modules
			NetworkworkN::<T>::insert(netuid, next_uid);

			// --- Increase total network count.
			TotalNetworks::<T>::mutate(|n| *n += 1);
		}
	}

	// ================
	// ==== Hooks =====
	// ================
  
	#[pallet::hooks] 
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> { 
		// ---- Called on the initialization of this pallet. (the order of on_finalize calls is determined in the runtime)
		//
		// # Args:
		// 	* 'n': (T::BlockNumber):
		// 		- The number of the block we are initializing.
		fn on_initialize( _block_number: BlockNumberFor<T> ) -> Weight {
			Self::block_step();
			
			return Weight::from_ref_time(110_634_229_000 as u64)
						.saturating_add(T::DbWeight::get().reads(8304 as u64))
						.saturating_add(T::DbWeight::get().writes(110 as u64));
		}
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {

		// --- Sets the caller weights for the incentive mechanism. The call can be
		// made from the key account so is potentially insecure, however, the damage
		// of changing weights is minimal if caught early. This function includes all the
		// checks that the passed weights meet the requirements. Stored as u16s they represent
		// rational values in the range [0,1] which sum to 1 and can be interpreted as
		// probabilities. The specific weights determine how inflation propagates outward
		// from this peer. 
		// 
		// Note: The 16 bit integers weights should represent 1.0 as the max u16.
		// However, the function normalizes all integers to u16_max anyway. This means that if the sum of all
		// elements is larger or smaller than the amount of elements * u16_max, all elements
		// will be corrected for this deviation. 
		// 
		// # Args:
		// 	* `origin`: (<T as frame_system::Config>Origin):
		// 		- The caller, a key who wishes to set their weights.
		//
		// 	* `netuid` (u16):
		// 		- The network uid we are setting these weights on.
		// 
		// 	* `dests` (Vec<u16>):
		// 		- The edge endpoint for the weight, i.e. j for w_ij.
		//
		// 	* 'weights' (Vec<u16>):
		// 		- The u16 integer encoded weights. Interpreted as rational
		// 		values in the range [0,1]. They must sum to in32::MAX.
		//
		// # Event:
		// 	* WeightsSet;
		// 		- On successfully setting the weights on chain.
		//
		// # Raises:
		// 	* 'NetworkDoesNotExist':
		// 		- Attempting to set weights on a non-existent network.
		//
		// 	* 'NotRegistered':
		// 		- Attempting to set weights from a non registered account.
		//
		// 	* 'WeightVecNotEqualSize':
		// 		- Attempting to set weights with uids not of same length.
		//
		// 	* 'DuplicateUids':
		// 		- Attempting to set weights with duplicate uids.
		//		
		//     * 'TooManyUids':
		// 		- Attempting to set weights above the max allowed uids.
		//
		// 	* 'InvalidUid':
		// 		- Attempting to set weights with invalid uids.
		//
		// 	* 'NotSettingEnoughWeights':
		// 		- Attempting to set weights with fewer weights than min.
		//
		// 	* 'MaxWeightExceeded':
		// 		- Attempting to set weights with max value exceeding limit.
        #[pallet::weight((Weight::from_ref_time(10_151_000_000)
		.saturating_add(T::DbWeight::get().reads(4104))
		.saturating_add(T::DbWeight::get().writes(2)), DispatchClass::Normal, Pays::No))]
		pub fn set_weights(
			origin:OriginFor<T>, 
			netuid: u16,
			dests: Vec<u16>, 
			weights: Vec<u16>,
		) -> DispatchResult {
			Self::do_set_weights( origin, netuid, dests, weights )
		}



		// --- Adds stake to a key. The call is made from the
		// key account linked in the key.
		// Only the associated key is allowed to make staking and
		// unstaking requests. This protects the module against
		// attacks on its key running in production code.
		//
		// # Args:
		// 	* 'origin': (<T as frame_system::Config>Origin):
		// 		- The signature of the caller's key .
		//
		// 	* 'key' (T::AccountId):
		// 		- The associated key account.
		//
		// 	* 'amount_staked' (u64):
		// 		- The amount of stake to be added to the key staking account.
		//
		// # Event:
		// 	* StakeAdded;
		// 		- On the successfully adding stake to a global account.
		//
		// # Raises:
		// 	* 'CouldNotConvertToBalance':
		// 		- Unable to convert the passed stake value to a balance.
		//
		// 	* 'NotEnoughBalanceToStake':
		// 		- Not enough balance on the key to add onto the global account.
		//
		// 	* 'BalanceWithdrawalError':
		// 		- Errors stemming from transaction pallet.
		//
		//
		#[pallet::weight((Weight::from_ref_time(65_000_000)
		.saturating_add(T::DbWeight::get().reads(8))
		.saturating_add(T::DbWeight::get().writes(6)), DispatchClass::Normal, Pays::No))]
		pub fn add_stake(
			origin: OriginFor<T>, 
			amount_staked: u64
		) -> DispatchResult {
			Self::do_add_stake(origin, amount_staked)
		}

		// ---- Remove stake from the staking account. The call must be made
		// from the key account attached to the module metadata. Only this key
		// has permission to make staking and unstaking requests.
		//
		// # Args:
		// 	* 'origin': (<T as frame_system::Config>Origin):
		// 		- The signature of the caller's key.
		//
		// 	* 'amount_unstaked' (u64):
		// 		- The amount of stake to be added to the key staking account.
		//
		// # Event:
		// 	* StakeRemoved;
		// 		- On the successfully removing stake from the key account.
		//
		// # Raises:
		// 	* 'NotRegistered':
		// 		- Thrown if the account we are attempting to unstake from is non existent.
		//

		// 	* 'NotEnoughStaketoWithdraw':
		// 		- Thrown if there is not enough stake on the key to withdwraw this amount. 
		//
		// 	* 'CouldNotConvertToBalance':
		// 		- Thrown if we could not convert this amount to a balance.
		//
		//
		#[pallet::weight((Weight::from_ref_time(66_000_000)
		.saturating_add(T::DbWeight::get().reads(8))
		.saturating_add(T::DbWeight::get().writes(6)), DispatchClass::Normal, Pays::No))]
		pub fn remove_stake(
			origin: OriginFor<T>, 
			key: T::AccountId, 
			amount_unstaked: u64
		) -> DispatchResult {
			Self::do_remove_stake(origin, amount_unstaked)
		}

		// ---- Serves or updates module /promethteus information for the module associated with the caller. If the caller is
		// already registered the metadata is updated. If the caller is not registered this call throws NotRegistered.
		//
		// # Args:
		// 	* 'origin': (<T as frame_system::Config>Origin):
		// 		- The signature of the caller.
		//
		// 	* 'netuid' (u16):
		// 		- The u16 network identifier.
		//
		// 	* 'version' (u64):
		// 		- The commune version identifier.
		//
		// 	* 'ip' (u64):
		// 		- The endpoint ip information as a u128 encoded integer.
		//
		// 	* 'port' (u16):
		// 		- The endpoint port information as a u16 encoded integer.
		// 
		// 	* 'ip_type' (u8):
		// 		- The endpoint ip version as a u8, 4 or 6.
		//
		// 	* 'protocol' (u8):
		// 		- UDP:1 or TCP:0 
		//
		// 	* 'placeholder1' (u8):
		// 		- Placeholder for further extra params.
		//
		// 	* 'placeholder2' (u8):
		// 		- Placeholder for further extra params.
		//
		// # Event:
		// 	* ModuleServed;
		// 		- On successfully serving the module info.
		//
		// # Raises:
		// 	* 'NetworkDoesNotExist':
		// 		- Attempting to set weights on a non-existent network.
		//
		// 	* 'NotRegistered':
		// 		- Attempting to set weights from a non registered account.
		//
		// 	* 'InvalidIpType':
		// 		- The ip type is not 4 or 6.
		//
		// 	* 'InvalidIpAddress':
		// 		- The numerically encoded ip address does not resolve to a proper ip.
		//
		// 	* 'ServingRateLimitExceeded':
		// 		- Attempting to set prometheus information withing the rate limit min.
		//
		#[pallet::weight((Weight::from_ref_time(19_000_000)
		.saturating_add(T::DbWeight::get().reads(2))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Normal, Pays::No))]
		pub fn serve_module(
			origin:OriginFor<T>, 
			netuid: u16,
			ip: u128, 
			port: u16, 
		) -> DispatchResult {
			Self::do_serve_module( origin, netuid, version, ip, port ) 
		}

		// ---- Registers a new module to the network. 
		//
		// # Args:
		// 	* 'origin': (<T as frame_system::Config>Origin):
		// 		- The signature of the calling key.
		//
		// 	* 'netuid' (u16):
		// 		- The u16 network identifier.
		//
		// 	* 'block_number' ( u64 ):
		// 		- Block hash used to prove work done.
		//
		// 	* 'nonce' ( u64 ):
		// 		- Positive integer nonce used in POW.
		//
		// 	* 'work' ( Vec<u8> ):
		// 		- Vector encoded bytes representing work done.
		//
		// 	* 'key' ( T::AccountId ):
		// 		- key to be registered to the network.
		//
		// 	* 'key' ( T::AccountId ):
		// 		- Associated key account.
		//
		// # Event:
		// 	* ModuleRegistered;
		// 		- On successfully registereing a uid to a module slot on a network.
		//
		// # Raises:
		// 	* 'NetworkDoesNotExist':
		// 		- Attempting to registed to a non existent network.
		//
		// 	* 'TooManyRegistrationsThisBlock':
		// 		- This registration exceeds the total allowed on this network this block.
		//
		// 	* 'AlreadyRegistered':
		// 		- The key is already registered on this network.
		//

		
		#[pallet::weight((Weight::from_ref_time(91_000_000)
		.saturating_add(T::DbWeight::get().reads(27))
		.saturating_add(T::DbWeight::get().writes(22)), DispatchClass::Normal, Pays::No))]
		pub fn register( 
				origin:OriginFor<T>, 
				netuid: u16,
		) -> DispatchResult { 
			// --- Disable registrations
			ensure!( false, Error::<T>::RegistrationDisabled ); 

			Self::do_registration(origin, netuid)
		}
		// ---- SUDO ONLY FUNCTIONS ------------------------------------------------------------



		// ==================================
		// ==== Parameter Sudo calls ========
		// ==================================
		// Each function sets the corresponding hyper paramter on the specified network
		// Args:
		// 	* 'origin': (<T as frame_system::Config>Origin):
		// 		- The caller, must be sudo.
		//
		// 	* `netuid` (u16):
		// 		- The network identifier.
		//
		// 	* `hyperparameter value` (u16):
		// 		- The value of the hyper parameter.
		//   

		#[pallet::weight((Weight::from_ref_time(10_000_000)
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_serving_rate_limit( origin:OriginFor<T>, netuid: u16, serving_rate_limit: u64 ) -> DispatchResult {  
			Self::do_sudo_set_serving_rate_limit( origin, netuid, serving_rate_limit )
		}

		// Sudo call for setting tx rate limit
		#[pallet::weight((0, DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_tx_rate_limit( origin:OriginFor<T>, tx_rate_limit: u64 ) -> DispatchResult {  
			Self::do_sudo_set_tx_rate_limit( origin, tx_rate_limit )
		}

		#[pallet::weight((Weight::from_ref_time(15_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_weights_set_rate_limit( origin:OriginFor<T>, netuid: u16, weights_set_rate_limit: u64 ) -> DispatchResult {  
			Self::do_sudo_set_weights_set_rate_limit( origin, netuid, weights_set_rate_limit )
		}
		#[pallet::weight((Weight::from_ref_time(14_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_bonds_moving_average( origin:OriginFor<T>, netuid: u16, bonds_moving_average: u64 ) -> DispatchResult {  
			Self::do_sudo_set_bonds_moving_average( origin, netuid, bonds_moving_average )
		}
		#[pallet::weight((Weight::from_ref_time(14_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_max_allowed_validators( origin:OriginFor<T>, netuid: u16, max_allowed_validators: u16 ) -> DispatchResult {  
			Self::do_sudo_set_max_allowed_validators( origin, netuid, max_allowed_validators )
		}
		#[pallet::weight((Weight::from_ref_time(13_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_difficulty( origin:OriginFor<T>, netuid: u16, difficulty: u64 ) -> DispatchResult {
			Self::do_sudo_set_difficulty( origin, netuid, difficulty )
		}
		#[pallet::weight((Weight::from_ref_time(14_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_adjustment_interval( origin:OriginFor<T>, netuid: u16, adjustment_interval: u16 ) -> DispatchResult { 
			Self::do_sudo_set_adjustment_interval( origin, netuid, adjustment_interval )
		}
		#[pallet::weight((Weight::from_ref_time(14_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_target_registrations_per_interval( origin:OriginFor<T>, netuid: u16, target_registrations_per_interval: u16 ) -> DispatchResult {
			Self::do_sudo_set_target_registrations_per_interval( origin, netuid, target_registrations_per_interval )
		}
		#[pallet::weight((Weight::from_ref_time(13_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_activity_cutoff( origin:OriginFor<T>, netuid: u16, activity_cutoff: u16 ) -> DispatchResult {
			Self::do_sudo_set_activity_cutoff( origin, netuid, activity_cutoff )
		}

		#[pallet::weight((Weight::from_ref_time(18_000_000)
		.saturating_add(T::DbWeight::get().reads(2))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_max_allowed_uids( origin:OriginFor<T>, netuid: u16, max_allowed_uids: u16 ) -> DispatchResult {
			Self::do_sudo_set_max_allowed_uids(origin, netuid, max_allowed_uids )
		}
		#[pallet::weight((Weight::from_ref_time(13_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_min_allowed_weights( origin:OriginFor<T>, netuid: u16, min_allowed_weights: u16 ) -> DispatchResult {
			Self::do_sudo_set_min_allowed_weights( origin, netuid, min_allowed_weights )
		}


		#[pallet::weight((Weight::from_ref_time(13_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_immunity_period( origin:OriginFor<T>, netuid: u16, immunity_period: u16 ) -> DispatchResult {
			Self::do_sudo_set_immunity_period( origin, netuid, immunity_period )
		}
		#[pallet::weight((Weight::from_ref_time(13_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_max_weight_limit( origin:OriginFor<T>, netuid: u16, max_weight_limit: u16 ) -> DispatchResult {
			Self::do_sudo_set_max_weight_limit( origin, netuid, max_weight_limit )
		}
		#[pallet::weight((Weight::from_ref_time(15_000_000)
		.saturating_add(T::DbWeight::get().reads(1))
		.saturating_add(T::DbWeight::get().writes(1)), DispatchClass::Operational, Pays::No))]
		pub fn sudo_set_max_registrations_per_block(origin: OriginFor<T>, netuid: u16, max_registrations_per_block: u16 ) -> DispatchResult {
			Self::do_sudo_set_max_registrations_per_block(origin, netuid, max_registrations_per_block )
		}


		// Benchmarking functions.
		#[pallet::weight((0, DispatchClass::Normal, Pays::No))]
		pub fn create_network( _: OriginFor<T>, netuid: u16, n: u16, tempo: u16 ) -> DispatchResult {
			Self::init_new_network( netuid, tempo, 1 );
			Self::set_max_allowed_uids( netuid, n );
			let mut seed : u32 = 1;
			for _ in 0..n {
				let block_number: u64 = Self::get_current_block_as_u64();
				let key: T::AccountId = T::AccountId::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap();
				Self::append_module( netuid, &key, block_number );
				seed = seed + 1;
			}
			Ok(())
		}

		#[pallet::weight((0, DispatchClass::Normal, Pays::No))]
		pub fn create_network_with_weights( _: OriginFor<T>, netuid: u16, n: u16, tempo: u16, n_vals: u16, n_weights: u16 ) -> DispatchResult {
			Self::init_new_network( netuid, tempo, 1 );
			Self::set_max_allowed_uids( netuid, n );
			Self::set_min_allowed_weights( netuid, n_weights );
			Self::set_emission_for_network( netuid, 1_000_000_000 );
			let mut seed : u32 = 1;
			for _ in 0..n {
				let block_number: u64 = Self::get_current_block_as_u64();
				let key: T::AccountId = T::AccountId::decode(&mut sp_runtime::traits::TrailingZeroInput::zeroes()).unwrap();
				Self::increase_stake_on_account( &key, 1_000_000_000 );
				Self::append_module( netuid, &key, block_number );
				seed = seed + 1;
			}
			for uid in 0..n {
				let uids: Vec<u16> = (0..n_weights).collect();
				let values: Vec<u16> = vec![1; n_weights as usize];
				let normalized_values = Self::normalize_weights( values );
				let mut zipped_weights: Vec<( u16, u16 )> = vec![];
				for ( uid, val ) in uids.iter().zip(normalized_values.iter()) { zipped_weights.push((*uid, *val)) }
				if uid < n_vals {
					Weights::<T>::insert( netuid, uid, zipped_weights );
				} else {
					break;
				}
			}
			Ok(())
		}

		#[pallet::weight((Weight::from_ref_time(49_882_000_000)
		.saturating_add(T::DbWeight::get().reads(8303))
		.saturating_add(T::DbWeight::get().writes(110)), DispatchClass::Normal, Pays::No))]
		pub fn benchmark_epoch_with_weights( _:OriginFor<T> ) -> DispatchResult {
			Self::epoch( 11, 1_000_000_000 );
			Ok(())
		} 
		#[pallet::weight((Weight::from_ref_time(117_586_465_000 as u64)
		.saturating_add(T::DbWeight::get().reads(12299 as u64))
		.saturating_add(T::DbWeight::get().writes(110 as u64)), DispatchClass::Normal, Pays::No))]
		pub fn benchmark_epoch_without_weights( _:OriginFor<T> ) -> DispatchResult {
			let _: Vec<(T::AccountId, u64)> = Self::epoch( 11, 1_000_000_000 );
			Ok(())
		} 
		#[pallet::weight((0, DispatchClass::Normal, Pays::No))]
		pub fn benchmark_drain_emission( _:OriginFor<T> ) -> DispatchResult {
			Self::drain_emission( 11 );
			Ok(())
		} 
	}	

	// ---- Subspace helper functions.
	impl<T: Config> Pallet<T> {
		// --- Returns the transaction priority for setting weights.
		pub fn get_priority_set_weights( key: &T::AccountId, netuid: u16 ) -> u64 {
			if Uids::<T>::contains_key( netuid, &key ) {
				let uid = Self::get_uid_for_net_and_key(netuid, &key.clone()).unwrap();
				let current_block_number: u64 = Self::get_current_block_as_u64();
				return current_block_number - Self::get_last_update_for_uid(netuid, uid as u16);
			}
			return 0;
		}
	}
}


/************************************************************
	CallType definition
************************************************************/
#[derive(Debug, PartialEq)]
pub enum CallType {
    SetWeights,
    AddStake,
    RemoveStake,
    Register,
    Serve,
	Other,
}
impl Default for CallType {
    fn default() -> Self {
        CallType::Other
    }
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
pub struct SubspaceSignedExtension<T: Config + Send + Sync + TypeInfo>(pub PhantomData<T>);

impl<T: Config + Send + Sync + TypeInfo> SubspaceSignedExtension<T> where
	T::RuntimeCall: Dispatchable<Info=DispatchInfo, PostInfo=PostDispatchInfo>,
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	pub fn new() -> Self {
		Self(Default::default())
	}

	pub fn get_priority_vanilla() -> u64 {
		// Return high priority so that every extrinsic except set_weights function will 
		// have a higher priority than the set_weights call
		return u64::max_value();
	}

	pub fn get_priority_set_weights( who: &T::AccountId, netuid: u16 ) -> u64 {
		// Return the non vanilla priority for a set weights call.

		return Pallet::<T>::get_priority_set_weights( who, netuid );
	}

	pub fn u64_to_balance( input: u64 ) -> Option<<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance> { input.try_into().ok() }

}

impl <T:Config + Send + Sync + TypeInfo> sp_std::fmt::Debug for SubspaceSignedExtension<T> {
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "SubspaceSignedExtension")
	}
}

impl<T: Config + Send + Sync + TypeInfo> SignedExtension for SubspaceSignedExtension<T>
    where
        T::RuntimeCall: Dispatchable<Info=DispatchInfo, PostInfo=PostDispatchInfo>,
        <T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	const IDENTIFIER: &'static str = "SubspaceSignedExtension";

	type AccountId = T::AccountId;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = (CallType, u64, Self::AccountId);
	
	fn additional_signed( &self ) -> Result<Self::AdditionalSigned, TransactionValidityError> { 
		Ok(())
	}


	fn validate(
		&self,
		who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		match call.is_sub_type() {
			Some(Call::set_weights{netuid, ..}) => {
				let priority: u64 = Self::get_priority_set_weights(who, *netuid);
                Ok(ValidTransaction {
                    priority: priority,
                    longevity: 1,
                    ..Default::default()
                })
            }
			Some(Call::add_stake{..}) => {
                Ok(ValidTransaction {
                    priority: Self::get_priority_vanilla(),
                    ..Default::default()
                })
            }
            Some(Call::remove_stake{..}) => {
                Ok(ValidTransaction {
                    priority: Self::get_priority_vanilla(),
                    ..Default::default()
                })
            }
            Some(Call::register{..}) => {
                Ok(ValidTransaction {
                    priority: Self::get_priority_vanilla(),
                    ..Default::default()
                })
            }
			_ => {
                Ok(ValidTransaction {
                    priority: Self::get_priority_vanilla(),
                    ..Default::default()
                })
            }
		}
	}

	// NOTE: Add later when we put in a pre and post dispatch step.
    fn pre_dispatch(
        self,
        who: &Self::AccountId,
        call: &Self::Call,
        _info: &DispatchInfoOf<Self::Call>,
        _len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {

        match call.is_sub_type() {
            Some(Call::add_stake{..}) => {
				let transaction_fee = 0;
                Ok((CallType::AddStake, transaction_fee, who.clone()))
            }
            Some(Call::remove_stake{..}) => {
				let transaction_fee = 0;
                Ok((CallType::RemoveStake, transaction_fee, who.clone()))
            }
			Some(Call::set_weights{..}) => {
				let transaction_fee = 0;
                Ok((CallType::SetWeights, transaction_fee, who.clone())) 
            }
			Some(Call::register{..}) => {
                let transaction_fee = 0;
                Ok((CallType::Register, transaction_fee, who.clone()))
            }
            Some(Call::serve_module{..}) => {
                let transaction_fee = 0;
                Ok((CallType::Serve, transaction_fee, who.clone()))
            }
            _ => {
				let transaction_fee = 0;
                Ok((CallType::Other, transaction_fee, who.clone()))
            }
        }
    }

	fn post_dispatch(
        maybe_pre: Option<Self::Pre>,
        _info: &DispatchInfoOf<Self::Call>,
        _post_info: &PostDispatchInfoOf<Self::Call>,
        _len: usize,
        _result: &dispatch::DispatchResult,
    ) -> Result<(), TransactionValidityError> {

		if let Some((call_type, _transaction_fee, _who)) = maybe_pre {
			match call_type {
				CallType::SetWeights => {
					log::debug!("Not Implemented!");
				}
				CallType::AddStake => {
					log::debug!("Not Implemented! Need to add potential transaction fees here.");
				}
				CallType::RemoveStake => {
					log::debug!("Not Implemented! Need to add potential transaction fees here.");
				}
				CallType::Register => {
					log::debug!("Not Implemented!");
				}
				_ => {
					log::debug!("Not Implemented!");
				}
			}
		} 
		Ok(())
    }

}
