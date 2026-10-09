//! QRZero core: the log database, ADIF and callsign lookup.

pub mod adif;
pub mod awards;
pub mod backup;
pub mod band;
pub mod confirm;
pub mod cwclubs;
pub mod cty;
pub mod error;
pub mod model;
pub mod qrz;
pub mod qsl;
pub mod secrets;
pub mod store;
pub mod worked;

pub use error::{Error, Result};
pub use store::Store;
