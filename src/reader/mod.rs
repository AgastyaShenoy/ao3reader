#![allow(unused_imports)]

pub mod font_manager;
pub mod paginator;
pub mod rich_text;
pub mod search;
pub mod theme;
pub mod typography;
#[cfg(feature = "legacy-ui")]
pub mod view;

pub use font_manager::{FontManager, SystemFont};
pub use paginator::{Page, PageItem, Paginator};
pub use search::ReaderSearchState;
#[cfg(feature = "legacy-ui")]
pub use theme::ReaderColors;
pub use theme::{Rgb, ThemePalette, ReaderThemeType};
pub use typography::{FontFamilyChoice, PageLayoutMode, ReadingMode, TextAlignment, TypographySettings};
#[cfg(feature = "legacy-ui")]
pub use view::{ReaderAction, ReaderView};
