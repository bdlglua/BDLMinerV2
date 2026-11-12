//! Hardware-independent BDLMiner firmware: everything between the mining
//! core and the board. The ESP32 firmware and the host build both run it.

pub mod app;
pub mod log;
pub mod miner;
pub mod multipart;
pub mod notify;
pub mod platform;
pub mod pool;
pub mod screen;
pub mod settings;
pub mod share;
pub mod std_platform;
pub mod ui_gamma;
pub mod ui_view;
pub mod update;
pub mod watch;
pub mod web;
pub mod web_validation;

pub use app::App;
