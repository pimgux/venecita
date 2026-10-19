//! The settings window: a UI-free view-model (this module's [`model`]) and the egui GUI
//! built on top of it (`gui`, macOS and Windows).
//!
//! The split is deliberate: everything that can be tested without a window — editing the
//! configuration, validating it on every change and saving it while keeping the user's
//! comments — lives in [`model::SettingsModel`] and is covered by unit tests. The egui
//! layer only turns widgets into calls on the model.

#[cfg(any(target_os = "macos", windows))]
pub mod gui;
pub mod keys;
pub mod model;
