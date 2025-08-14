
#![no_std]
#![allow(static_mut_refs)]

use sails_rs::{
    prelude::*,
    gstd::msg,
    collections::HashMap,
};

// ============================ vNFT CONTRACT STATE ============================
static mut VNFT_STATE: Option<VnftState> = None;

/// Struct representing a single NFT.
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct NFT {
    pub id: u64,
    pub owner: ActorId,
    pub metadata: String,
}

/// State for the vNFT contract.
#[derive(Debug, Default, Clone)]
pub struct VnftState {
    pub admin: ActorId,
    pub nfts: HashMap<u64, NFT>,
    pub owner_nfts: HashMap<ActorId, Vec<u64>>,
    pub next_id: u64,
    pub main_contract: Option<ActorId>,
}

impl VnftState {
    /// Initialize state: required to call from seed function.
    pub fn init(admin: ActorId, main_contract: Option<ActorId>) {
        unsafe {
            VNFT_STATE = Some(Self {
                admin,
                nfts: HashMap::new(),
                owner_nfts: HashMap::new(),
                next_id: 1,
                main_contract,
            });
        }
    }
    /// Get mutable ref to state.
    pub fn state_mut() -> &'static mut Self {
        let state = unsafe { VNFT_STATE.as_mut() };
        debug_assert!(state.is_some(), "State uninitialized");
        unsafe { state.unwrap_unchecked() }
    }
    /// Get immutable ref to state.
    pub fn state_ref() -> &'static Self {
        let state = unsafe { VNFT_STATE.as_ref() };
        debug_assert!(state.is_some(), "State uninitialized");
        unsafe { state.unwrap_unchecked() }
    }
}

/// Output struct for state query.
#[derive(Debug, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct IoVnftState {
    pub admin: ActorId,
    pub nfts: Vec<NFT>,
    pub main_contract: Option<ActorId>,
}

impl From<VnftState> for IoVnftState {
    fn from(state: VnftState) -> Self {
        Self {
            admin: state.admin,
            nfts: state.nfts.values().cloned().collect(),
            main_contract: state.main_contract,
        }
    }
}

// ============================ vNFT EVENTS AND ERRORS =========================

#[derive(Debug, Encode, Decode, TypeInfo, Clone)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub enum VnftEvent {
    Minted { id: u64, owner: ActorId },
    Burned { id: u64, owner: ActorId },
    Transferred { id: u64, from: ActorId, to: ActorId },
    MainContractSet(ActorId),
}

#[derive(Debug, Encode, Decode, TypeInfo, Clone)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub enum VnftError {
    Unauthorized,
    NotFound,
    NotOwner,
    AlreadyExists,
    InvalidMainContract,
    Overflow,
}

// ============================ vNFT SERVICE ===================================
#[derive(Default)]
pub struct Service;

impl Service {
    /// Seed the contract: set admin and optionally main contract.
    pub fn seed(admin: ActorId, main_contract: Option<ActorId>) {
        if admin == ActorId::zero() {
            panic!("Admin cannot be zero");
        }
        VnftState::init(admin, main_contract);
    }
}

#[sails_rs::service(events = VnftEvent)]
impl Service {
    /// Constructor (not used except by macro).
    pub fn new() -> Self {
        Self
    }

    /// Mint a new NFT. Only admin or main_contract can mint.
    pub fn mint(&mut self, to: ActorId, metadata: String) -> VnftEvent {
        let caller = msg::source();
        let mut state = VnftState::state_mut();
        let may_main = state.main_contract;
        let is_admin = caller == state.admin;
        let is_main = may_main.filter(|id| *id == caller).is_some();
        if !is_admin && !is_main {
            panic!("Not authorized");
        }
        let new_id = state.next_id;
        state.next_id = state.next_id.checked_add(1).expect("Overflow");
        let nft = NFT { id: new_id, owner: to, metadata: metadata.clone() };
        state.nfts.insert(new_id, nft.clone());
        state.owner_nfts.entry(to).or_insert(Vec::new()).push(new_id);
        self.emit_event(VnftEvent::Minted { id: new_id, owner: to }).expect("Notification failure");
        VnftEvent::Minted { id: new_id, owner: to }
    }

    /// Burn an NFT. Only owner can burn.
    pub fn burn(&mut self, id: u64) -> VnftEvent {
        let caller = msg::source();
        let mut state = VnftState::state_mut();
        let nft = state.nfts.get(&id).cloned().expect("NFT not found");
        if nft.owner != caller {
            panic!("Only owner can burn");
        }
        state.nfts.remove(&id);
        if let Some(owned) = state.owner_nfts.get_mut(&caller) {
            owned.retain(|x| *x != id);
        }
        self.emit_event(VnftEvent::Burned { id, owner: caller }).expect("Notification failure");
        VnftEvent::Burned { id, owner: caller }
    }

    /// Transfer an NFT to another user.
    pub fn transfer(&mut self, id: u64, to: ActorId) -> VnftEvent {
        let caller = msg::source();
        let mut state = VnftState::state_mut();
        let nft = state.nfts.get_mut(&id).expect("NFT not found");
        if nft.owner != caller {
            panic!("Only owner can transfer");
        }
        let from = nft.owner;
        nft.owner = to;
        // Update old owner's list
        if let Some(owned) = state.owner_nfts.get_mut(&from) {
            owned.retain(|x| *x != id);
        }
        state.owner_nfts.entry(to).or_insert(Vec::new()).push(id);
        self.emit_event(VnftEvent::Transferred { id, from, to }).expect("Notification failure");
        VnftEvent::Transferred { id, from, to }
    }

    /// Set address of Main Contract allowed for cross-contract minting etc. Only admin.
    pub fn set_main_contract(&mut self, main_contract: ActorId) -> VnftEvent {
        let caller = msg::source();
        let mut state = VnftState::state_mut();
        if caller != state.admin {
            panic!("Only admin can set main contract");
        }
        state.main_contract = Some(main_contract);
        self.emit_event(VnftEvent::MainContractSet(main_contract)).expect("Notification failure");
        VnftEvent::MainContractSet(main_contract)
    }

    // ============================ QUERIES (3+) ===================================
    /// Query NFT info by id.
    pub fn query_nft(&self, id: u64) -> Option<NFT> {
        VnftState::state_ref().nfts.get(&id).cloned()
    }

    /// Query all NFTs of a user.
    pub fn query_nfts_of(&self, owner: ActorId) -> Vec<NFT> {
        let state = VnftState::state_ref();
        let ids = state.owner_nfts.get(&owner);
        match ids {
            Some(vec) => vec.iter().filter_map(|id| state.nfts.get(id).cloned()).collect(),
            None => Vec::new(),
        }
    }

    /// Query contract state (admin, NFTs, main contract)
    pub fn query_state(&self) -> IoVnftState {
        VnftState::state_ref().clone().into()
    }

    /// Query main contract address
    pub fn query_main_contract(&self) -> Option<ActorId> {
        VnftState::state_ref().main_contract
    }

    /// Returns the list of NFT ids owned by a specific user.
    pub fn tokens_for_owner(&self, owner: ActorId) -> Vec<u64> {
        let state = VnftState::state_ref();
        match state.owner_nfts.get(&owner) {
            Some(ids) => ids.clone(),
            None => Vec::new(),
        }
    }

    /// Returns the list of all NFTs available with their token_id and token_metadata.
    pub fn available_nfts(&self) -> Vec<(u64, String)> {
        VnftState::state_ref()
            .nfts
            .values()
            .map(|nft| (nft.id, nft.metadata.clone()))
            .collect()
    }
}
