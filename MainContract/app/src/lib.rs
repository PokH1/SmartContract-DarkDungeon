
#![no_std]
use sails_rs::prelude::*;
pub mod services;
use services::service::Service;

pub struct Program;

#[program]
impl Program {
    pub fn new() -> Self {
        Service::seed();
        Self
    }

    #[route("Service")]
    pub fn service(&self) -> Service {
        Service::new()
    }
}
