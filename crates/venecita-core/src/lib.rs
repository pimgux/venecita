//! Platform-independent core of Venecita Dock.
//!
//! Everything here is plain Rust with no OS calls, so it can be unit tested on any machine:
//!
//! * [`geometry`]: rectangles and directions.
//! * [`grid`]: the `cols × rows` grid, zones and snapping.
//! * [`display`]: connected displays and matching them to the user's definitions.
//! * [`rules`]: identifying applications.
//! * [`config`]: the `config.toml` schema and validation.
//! * [`profile`]: choosing the profile/layout for the connected displays.
//! * [`action`]: the actions every input method (hotkeys, tray, API, CLI) triggers.
//! * [`window`]: windows and platform events.
//! * [`stack`]: windows sharing a zone.
//! * [`engine`]: the placement state machine tying everything together.
//!
//! See `docs/ARCHITECTURE.md` for the big picture.

#![forbid(unsafe_code)]

pub mod action;
pub mod config;
pub mod display;
pub mod engine;
pub mod geometry;
pub mod grid;
pub mod profile;
pub mod rules;
pub mod stack;
pub mod window;

pub use action::{Action, Hotkey};
pub use config::Config;
pub use geometry::{Direction, Rect};
pub use grid::{GridSize, Zone};
