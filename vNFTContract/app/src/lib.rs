
#![no_std]
use sails_rs::prelude::*;
pub mod services;
use services::service::Service;

pub struct Program;

#[program]
impl Program {
    pub fn new(admin: ActorId, main_contract: Option<ActorId>) -> Self {
        Service::seed(admin, main_contract);
        Self
    }

    #[route("Service")]
    pub fn service(&self) -> Service {
        Service::new()
    }
}
