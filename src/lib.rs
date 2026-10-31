#![allow(dead_code)]
#![allow(unused_imports)]

pub mod ao3;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod reader;
pub mod sources;
pub mod storage;
pub mod updater;

#[cfg(all(windows, feature = "native-ui"))]
pub mod native;

#[cfg(feature = "legacy-ui")]
pub mod app;
#[cfg(feature = "legacy-ui")]
pub mod ui;

#[cfg(feature = "legacy-ui")]
pub use app::AO3ReaderApp;
