#![no_std]
use sails_rs::prelude::*;
pub mod services;

use services::service::Service;

pub struct Program;

#[program]
impl Program {
    pub fn new(admin: ActorId, main_contract: Option<ActorId>, gas_for_one_time_updating: u64) -> Self {
        Service::seed(admin, main_contract, gas_for_one_time_updating);
        Self
    }

    #[route("Service")]
    pub fn service(&self) -> Service {
        Service::new()
    }
}