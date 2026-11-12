//! Hardware-independent BDLMiner firmware: everything between the mining
//! core and the board. The ESP32 firmware and the host build both run it.

pub mod app;
pub mod block_status;
pub mod crash;
pub mod json_stream;
pub mod log;
pub mod market;
pub mod miner;
pub mod mqtt;
pub mod multipart;
pub mod notify;
pub mod platform;
pub mod pool;
pub mod remote;
pub mod screen;
pub mod settings;
pub mod share;
pub mod std_platform;
pub mod summary;
pub mod ui_gamma;
pub mod ui_view;
pub mod update;
pub mod watch;
pub mod web;
pub mod web_validation;

pub use app::App;
