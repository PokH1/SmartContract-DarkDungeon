
#![no_std]
#![allow(static_mut_refs)]

use sails_rs::{
    prelude::*,
    gstd::{msg, exec, ext},
    collections::HashMap,
    scale_codec::{Encode, Decode},
};
use core::fmt::Debug;

// ============================ vNFT CONTRACT STATE ============================
static mut VNFT_STATE: Option<VnftState> = None;

/// Metadata for a single NFT, supporting dynamic media.
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct TokenMetadata {
    pub name: String,
    pub description: String,
    pub current_media_index: u64,
    pub media: Vec<String>,
    pub reference: String,
}

/// Struct representing a single NFT.
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
#[codec(crate = sails_rs::scale_codec)]
#[scale_info(crate = sails_rs::scale_info)]
pub struct NFT {
    pub id: u64,
    pub owner: ActorId,
    pub metadata: TokenMetadata,
}

/// State for the vNFT contract.
#[derive(Debug, Default, Clone)]
pub struct VnftState {
    pub admin: ActorId,
    pub nfts: HashMap<u64, NFT>,
    pub owner_nfts: HashMap<ActorId, Vec<u64>>,
    pub next_id: u64,
    pub main_contract: Option<ActorId>,
    pub gas_for_one_time_updating: u64,
}

impl VnftState {
    /// Initialize state: required to call from seed function.
    pub fn init(admin: ActorId, main_contract: Option<ActorId>, gas_for_one_time_updating: u64) {
        unsafe {
            VNFT_STATE = Some(Self {
                admin,
                nfts: HashMap::new(),
                owner_nfts: HashMap::new(),
                next_id: 1,
                main_contract,
                gas_for_one_time_updating,
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
    pub gas_for_one_time_updating: u64,
}

impl From<VnftState> for IoVnftState {
    fn from(state: VnftState) -> Self {
        Self {
            admin: state.admin,
            nfts: state.nfts.values().cloned().collect(),
            main_contract: state.main_contract,
            gas_for_one_time_updating: state.gas_for_one_time_updating,
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
    MetadataStartedUpdating { updates_count: u32, update_period_in_blocks: u32, token_id: u64 },
    MetadataUpdated { token_id: u64, current_media_index: u64 },
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
    TokenDoesNotExist,
    DeniedAccess,
    InvalidUpdateCount,
    InvalidUpdatePeriod,
    NotificationError,
    OnlyProgramCanUpdate,
}

// ============================ vNFT SERVICE ===================================
#[derive(Default)]
pub struct Service;

impl Service {
    /// Seed the contract: set admin and optionally main contract.
    pub fn seed(admin: ActorId, main_contract: Option<ActorId>, gas_for_one_time_updating: u64) {
        if admin == ActorId::zero() {
            panic!("Admin cannot be zero");
        }
        VnftState::init(admin, main_contract, gas_for_one_time_updating);
    }
}

#[sails_rs::service(events = VnftEvent)]
impl Service {
    /// Constructor (not used except by macro).
    pub fn new() -> Self {
        Self
    }

    /// Mint a new NFT. Only admin or main_contract can mint.
    pub fn mint(&mut self, to: ActorId, metadata: TokenMetadata) -> VnftEvent {
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

    // ============================ DYNAMIC METADATA EXTENSION ============================
    /// Start scheduled metadata update for a token.
    pub fn start_metadata_update(
        &mut self,
        updates_count: u32,
        update_period_in_blocks: u32,
        token_id: u64,
    ) -> VnftEvent {
        let msg_src = msg::source();
        if updates_count == 0 {
            panic!("Updates count cannot be zero");
        }
        if update_period_in_blocks == 0 {
            panic!("Updates period cannot be zero");
        }
        panicking(|| {
            start_metadata_updates(
                VnftState::state_ref().gas_for_one_time_updating,
                &mut VnftState::state_mut().nfts,
                &mut VnftState::state_mut().owner_nfts,
                token_id,
                msg_src,
                updates_count,
                update_period_in_blocks,
            )
        });
        self.emit_event(VnftEvent::MetadataStartedUpdating {
            updates_count,
            update_period_in_blocks,
            token_id,
        }).expect("Notification Error");
        VnftEvent::MetadataStartedUpdating {
            updates_count,
            update_period_in_blocks,
            token_id,
        }
    }

    /// Handle actual metadata update for a token (called by delayed message).
    pub fn update_metadata(
        &mut self,
        token_id: u64,
        owner: ActorId,
        update_period: u32,
        updates_count: u32,
    ) -> VnftEvent {
        if msg::source() != exec::program_id() {
            panic!("This message can only be sent by the programme");
        }
        let current_media_index = panicking(|| {
            updates_metadata(
                &mut VnftState::state_mut().nfts,
                &mut VnftState::state_mut().owner_nfts,
                token_id,
                owner,
                update_period,
                updates_count,
            )
        });
        self.emit_event(VnftEvent::MetadataUpdated { token_id, current_media_index })
            .expect("Notification Error");
        VnftEvent::MetadataUpdated { token_id, current_media_index }
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
    pub fn available_nfts(&self) -> Vec<(u64, TokenMetadata)> {
        VnftState::state_ref()
            .nfts
            .values()
            .map(|nft| (nft.id, nft.metadata.clone()))
            .collect()
    }
}

// ============================ DYNAMIC METADATA LOGIC ============================

pub fn start_metadata_updates(
    gas_for_one_time_updating: u64,
    nfts: &mut HashMap<u64, NFT>,
    owner_nfts: &mut HashMap<ActorId, Vec<u64>>,
    token_id: u64,
    msg_src: ActorId,
    updates_count: u32,
    update_period: u32,
) -> Result<(), VnftError> {
    let nft = nfts.get_mut(&token_id).ok_or(VnftError::TokenDoesNotExist)?;
    if nft.owner != msg_src {
        return Err(VnftError::DeniedAccess);
    }
    let metadata = &mut nft.metadata;
    let media_len = metadata.media.len() as u64;
    if media_len == 0 {
        return Err(VnftError::TokenDoesNotExist);
    }
    metadata.current_media_index = metadata.current_media_index.saturating_add(1) % media_len;
    if updates_count.saturating_sub(1) != 0 {
        let request = [
            b"DynamicNft".encode(),
            b"UpdateMetadata".encode(),
            (token_id, msg_src, update_period, updates_count - 1).encode(),
        ]
        .concat();
        msg::send_bytes_with_gas_delayed(
            exec::program_id(),
            request,
            gas_for_one_time_updating.saturating_mul(updates_count.into()),
            0,
            update_period,
        )
        .expect("Error in sending message");
    }
    Ok(())
}

pub fn updates_metadata(
    nfts: &mut HashMap<u64, NFT>,
    owner_nfts: &mut HashMap<ActorId, Vec<u64>>,
    token_id: u64,
    owner: ActorId,
    update_period: u32,
    updates_count: u32,
) -> Result<u64, VnftError> {
    let nft = nfts.get_mut(&token_id).ok_or(VnftError::TokenDoesNotExist)?;
    if nft.owner != owner {
        return Err(VnftError::DeniedAccess);
    }
    let metadata = &mut nft.metadata;
    let media_len = metadata.media.len() as u64;
    if media_len == 0 {
        return Err(VnftError::TokenDoesNotExist);
    }
    metadata.current_media_index = metadata.current_media_index.saturating_add(1) % media_len;
    if updates_count.saturating_sub(1) != 0 {
        let request = [
            b"DynamicNft".encode(),
            b"UpdateMetadata".encode(),
            (token_id, owner, update_period, updates_count - 1).encode(),
        ]
        .concat();
        msg::send_bytes_with_gas_delayed(
            exec::program_id(),
            request,
            exec::gas_available().saturating_sub(1_000_000_000),
            0,
            update_period,
        )
        .expect("Error in sending message");
    }
    Ok(metadata.current_media_index)
}

pub fn panicking<T, E: Debug, F: FnOnce() -> Result<T, E>>(f: F) -> T {
    match f() {
        Ok(v) => v,
        Err(e) => panic!("{:?}", e),
    }
}

pub fn panic(err: impl Debug) -> ! {
    ext::panic(format!("{err:?}"))
}