
#![no_std]
#![allow(static_mut_refs)]

use sails_rs::{
    prelude::*,
    gstd::msg,
    collections::HashMap,
};

pub static mut MAIN_CONTRACT_STATE: Option<MainContractState> = None;

/// Represents state for each user and their selected NFTs
#[derive(Debug, Encode, Decode, TypeInfo, Clone)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct UserSelection {
    pub user: ActorId,
    pub selected_nfts: Vec<(ActorId, U256)>, // (nft_contract_id, token_id)
}

/// Stats for a finished run
#[derive(Debug, Encode, Decode, TypeInfo, Clone)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct RunStats {
    pub user: ActorId,
    pub monsters_defeated: u32,
    pub items_found: Vec<U256>,
    pub new_items_selected: Vec<U256>,
    pub survival_time: u64,
}

/// Keyring entry for a user
#[derive(Debug, Encode, Decode, TypeInfo, Clone)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct KeyringEntry {
    pub user: ActorId,
    pub public_key: Vec<u8>,
    pub metadata: Option<String>,
}

/// State for MainContract
#[derive(Debug, Clone, Default)]
pub struct MainContractState {
    pub admins: Vec<ActorId>,
    pub user_nft_selections: HashMap<ActorId, Vec<(ActorId, U256)>>,
    pub last_run_stats: HashMap<ActorId, RunStats>,
    pub keyring: HashMap<ActorId, KeyringEntry>,
}

impl MainContractState {
    pub fn init() {
        unsafe {
            MAIN_CONTRACT_STATE = Some(Self {
                admins: vec![msg::source()],
                ..Default::default()
            });
        }
    }
    pub fn state_mut() -> &'static mut MainContractState {
        let state = unsafe { MAIN_CONTRACT_STATE.as_mut() };
        debug_assert!(state.is_some(), "MainContract not initialized");
        unsafe { state.unwrap_unchecked() }
    }
    pub fn state_ref() -> &'static MainContractState {
        let state = unsafe { MAIN_CONTRACT_STATE.as_ref() };
        debug_assert!(state.is_some(), "MainContract not initialized");
        unsafe { state.unwrap_unchecked() }
    }
}

/// Events for MainContract
#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub enum MainEvent {
    NFTsSelected {
        user: ActorId,
        selected_nfts: Vec<(ActorId, U256)>,
    },
    AdminAdded(ActorId),
    RunStarted {
        initiator: ActorId,
        participants: Vec<(ActorId, Vec<(ActorId, U256)>)>,
        signature: [u8; 32],
    },
    RunFinished {
        user: ActorId,
        monsters_defeated: u32,
        items_found: Vec<U256>,
        new_items_selected: Vec<U256>,
        survival_time: u64,
    },
    NewWeaponSelected {
        user: ActorId,
        token_id: U256,
    },
    KeyAdded {
        user: ActorId,
        public_key: Vec<u8>,
        metadata: Option<String>,
    },
    KeyRemoved {
        user: ActorId,
    },
    KeyUpdated {
        user: ActorId,
        public_key: Vec<u8>,
        metadata: Option<String>,
    },
}

/// Queryable IoState
#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct IoMainContractState {
    pub admins: Vec<ActorId>,
    pub user_nft_selections: Vec<UserSelection>,
    pub last_run_stats: Vec<RunStats>,
    pub keyring: Vec<KeyringEntry>,
}

impl From<MainContractState> for IoMainContractState {
    fn from(state: MainContractState) -> Self {
        let user_nft_selections = state.user_nft_selections
            .iter()
            .map(|(k, v)| UserSelection {
                user: *k,
                selected_nfts: v.clone(),
            })
            .collect();
        let last_run_stats = state.last_run_stats
            .values()
            .cloned()
            .collect();
        let keyring = state.keyring
            .values()
            .cloned()
            .collect();
        IoMainContractState {
            admins: state.admins,
            user_nft_selections,
            last_run_stats,
            keyring,
        }
    }
}

#[derive(Default)]
pub struct Service;

impl Service {
    /// Seeds the contract - MUST be called ONCE after deploy
    pub fn seed() {
        MainContractState::init();
    }
}

#[sails_rs::service(events = MainEvent)]
impl Service {
    pub fn new() -> Self { Self }

    /// User selects which NFTs wants to use
    /// Each NFT selection is a tuple: (nft_contract_id, token_id)
    pub fn nfts_selected_by_user(&mut self, selected_nfts: Vec<(ActorId, U256)>) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let user = msg::source();

        // Validation: no duplicates, no empty
        if selected_nfts.is_empty() {
            panic!("NFT selection can't be empty");
        }
        // Optionally: only unique NFTs
        let mut seen: Vec<(ActorId, U256)> = Vec::with_capacity(selected_nfts.len());
        for (cid, tid) in &selected_nfts {
            if seen.contains(&(*cid, *tid)) {
                panic!("Duplicate NFT selection");
            }
            seen.push((*cid, *tid));
        }
        // Store the selection for the user, overwrite previous selection
        state.user_nft_selections.insert(user, selected_nfts.clone());

        self.emit_event(MainEvent::NFTsSelected {
            user,
            selected_nfts
        }).expect("Failed to emit event");

        MainEvent::NFTsSelected {
            user,
            selected_nfts,
        }
    }

    /// Adds an admin (only admins can do this)
    pub fn add_admin(&mut self, new_admin: ActorId) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let caller = msg::source();
        if !state.admins.contains(&caller) {
            panic!("Only admins can add new admins");
        }
        if !state.admins.contains(&new_admin) {
            state.admins.push(new_admin);
        }
        self.emit_event(MainEvent::AdminAdded(new_admin)).expect("Failed to emit event");
        MainEvent::AdminAdded(new_admin)
    }

    /// Query: gets NFTs selected by a user (returns only the token IDs)
    pub fn query_user_nfts(&self, user: ActorId) -> Option<Vec<U256>> {
        MainContractState::state_ref()
            .user_nft_selections
            .get(&user)
            .map(|nfts| nfts.iter().map(|(_cid, tid)| *tid).collect())
    }

    /// Query: get all users which have selected NFTs and their selections
    pub fn query_all_user_selections(&self) -> Vec<UserSelection> {
        MainContractState::state_ref()
            .user_nft_selections
            .iter()
            .map(|(k, v)| UserSelection {
                user: *k,
                selected_nfts: v.clone(),
            })
            .collect()
    }

    /// Query: returns all admins
    pub fn query_admins(&self) -> Vec<ActorId> {
        MainContractState::state_ref().admins.clone()
    }

    /// Query: returns full contract state
    pub fn query_state(&self) -> IoMainContractState {
        MainContractState::state_ref().clone().into()
    }

    /// Starts a run/game session and signs the creation so the player can start
    pub fn start_run(&mut self) -> MainEvent {
        let state = MainContractState::state_ref();
        let initiator = msg::source();

        // Gather all participants and their NFT selections
        let participants: Vec<(ActorId, Vec<(ActorId, U256)>)> = state
            .user_nft_selections
            .iter()
            .map(|(user, nfts)| (*user, nfts.clone()))
            .collect();

        if participants.is_empty() {
            panic!("No users have selected NFTs. Cannot start run.");
        }

        // Create a hash/signature of the run creation for proof
        let mut hash_data = Vec::new();
        hash_data.extend_from_slice(&initiator.encode());
        for (user, nfts) in &participants {
            hash_data.extend_from_slice(&user.encode());
            for (cid, tid) in nfts {
                hash_data.extend_from_slice(&cid.encode());
                hash_data.extend_from_slice(&tid.encode());
            }
        }
        let signature = sails_rs::prelude::hash::blake2b_256(&hash_data);

        self.emit_event(MainEvent::RunStarted {
            initiator,
            participants: participants.clone(),
            signature,
        }).expect("Failed to emit event");

        MainEvent::RunStarted {
            initiator,
            participants,
            signature,
        }
    }

    /// Called when a run finishes to submit stats
    pub fn finish_run(
        &mut self,
        monsters_defeated: u32,
        items_found: Vec<U256>,
        new_items_selected: Vec<U256>,
        survival_time: u64,
    ) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let user = msg::source();

        // Simple anti-cheat: stats must be in reasonable ranges
        if monsters_defeated > 1000 {
            panic!("Invalid stats: too many monsters defeated");
        }
        if items_found.len() > 100 {
            panic!("Invalid stats: too many items found");
        }
        if new_items_selected.len() > 100 {
            panic!("Invalid stats: too many new items selected");
        }
        if survival_time > 1000 * 60 * 60 * 24 {
            panic!("Invalid stats: survival time too high");
        }

        let stats = RunStats {
            user,
            monsters_defeated,
            items_found: items_found.clone(),
            new_items_selected: new_items_selected.clone(),
            survival_time,
        };
        state.last_run_stats.insert(user, stats.clone());

        self.emit_event(MainEvent::RunFinished {
            user,
            monsters_defeated,
            items_found,
            new_items_selected,
            survival_time,
        }).expect("Failed to emit event");

        MainEvent::RunFinished {
            user,
            monsters_defeated,
            items_found,
            new_items_selected,
            survival_time,
        }
    }

    /// Sets a new selected weapon for the user
    pub fn set_new_selected_weapon(&mut self, token_id: U256) -> MainEvent {
        let user = msg::source();

        self.emit_event(MainEvent::NewWeaponSelected {
            user,
            token_id,
        }).expect("Failed to emit event");

        MainEvent::NewWeaponSelected {
            user,
            token_id,
        }
    }

    /// Returns the new stats to the user after the run finishes
    pub fn run_finished(&mut self, new_status: RunStats) -> RunStats {
        // Anti-cheat validation
        if new_status.monsters_defeated > 1000 {
            panic!("Invalid stats: too many monsters defeated");
        }
        if new_status.items_found.len() > 100 {
            panic!("Invalid stats: too many items found");
        }
        if new_status.new_items_selected.len() > 100 {
            panic!("Invalid stats: too many new items selected");
        }
        if new_status.survival_time > 1000 * 60 * 60 * 24 {
            panic!("Invalid stats: survival time too high");
        }

        let mut state = MainContractState::state_mut();
        let user = msg::source();

        // Store the new stats for the user
        state.last_run_stats.insert(user, new_status.clone());

        // Return the new stats to the user
        new_status
    }

    /// Keyring: Add a public key for the user
    pub fn add_key(&mut self, public_key: Vec<u8>, metadata: Option<String>) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let user = msg::source();

        if state.keyring.contains_key(&user) {
            panic!("Key already exists for user");
        }

        let entry = KeyringEntry {
            user,
            public_key: public_key.clone(),
            metadata: metadata.clone(),
        };
        state.keyring.insert(user, entry);

        self.emit_event(MainEvent::KeyAdded {
            user,
            public_key,
            metadata,
        }).expect("Failed to emit event");

        MainEvent::KeyAdded {
            user,
            public_key,
            metadata,
        }
    }

    /// Keyring: Remove the public key for the user
    pub fn remove_key(&mut self) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let user = msg::source();

        if !state.keyring.contains_key(&user) {
            panic!("No key exists for user");
        }

        state.keyring.remove(&user);

        self.emit_event(MainEvent::KeyRemoved {
            user,
        }).expect("Failed to emit event");

        MainEvent::KeyRemoved {
            user,
        }
    }

    /// Keyring: Update the public key for the user
    pub fn update_key(&mut self, public_key: Vec<u8>, metadata: Option<String>) -> MainEvent {
        let mut state = MainContractState::state_mut();
        let user = msg::source();

        if !state.keyring.contains_key(&user) {
            panic!("No key exists for user");
        }

        let entry = KeyringEntry {
            user,
            public_key: public_key.clone(),
            metadata: metadata.clone(),
        };
        state.keyring.insert(user, entry);

        self.emit_event(MainEvent::KeyUpdated {
            user,
            public_key,
            metadata,
        }).expect("Failed to emit event");

        MainEvent::KeyUpdated {
            user,
            public_key,
            metadata,
        }
    }

    /// Keyring: Query the public key for a user
    pub fn query_key(&self, user: ActorId) -> Option<KeyringEntry> {
        MainContractState::state_ref()
            .keyring
            .get(&user)
            .cloned()
    }

    /// Keyring: Query all keyring entries
    pub fn query_all_keys(&self) -> Vec<KeyringEntry> {
        MainContractState::state_ref()
            .keyring
            .values()
            .cloned()
            .collect()
    }
}
