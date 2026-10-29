#![allow(dead_code)]
#![allow(unused_imports)]

pub mod ao3;
pub mod reader;
pub mod storage;
pub mod updater;

#[cfg(feature = "legacy-ui")]
pub mod app;
#[cfg(feature = "legacy-ui")]
pub mod ui;

#[cfg(feature = "legacy-ui")]
pub use app::AO3ReaderApp;
