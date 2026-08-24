//! Chip-independent Matter services for Ariel OS.
//!
//! The crate contains only portable state machines, typed device APIs, and
//! platform traits. Chip drivers and the `rs-matter` protocol engine stay in
//! separate crates.

#![no_std]
#![forbid(unsafe_code)]
#![allow(async_fn_in_trait)]

#[cfg(test)]
extern crate std;

pub mod access;
pub mod bridge;
pub mod closure;
pub mod devices;
pub mod endpoint;
pub mod error;
pub mod factory_reset;
pub mod ota;
pub mod platform;
pub mod runtime;
pub mod storage;
pub mod wifi;

pub use error::{Error, Result};
