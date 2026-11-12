//! Portable BDLMiner mining core, shared by the ESP32 firmware and the host
//! build. Nothing here touches hardware: everything is unit-tested on a PC.

// Indexed loops follow the published algorithms (SHA-256 rounds, wire layouts).
#![allow(clippy::needless_range_loop)]

pub mod address;
pub mod bm1370;
pub mod bm1397;
pub mod build_time;
pub mod coin;
pub mod display_config;
pub mod fan_control;
pub mod fill;
pub mod image_marker;
pub mod line_buffer;
pub mod pool_agent;
pub mod pool_diagnostics;
pub mod pool_identity;
pub mod release_version;
pub mod remote_data;
pub mod safety;
pub mod sha256;
pub mod share_reason;
pub mod stratum;
pub mod sv2_key;
#[cfg(feature = "sv2-noise")]
pub mod sv2_noise;
pub mod sv2_protocol;
pub mod work;

pub use coin::Coin;
