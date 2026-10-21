//! Venecita Dock's local API: what other programs (a stream deck, a macro pad, scripts, the
//! `venecita` CLI) use to control it.
//!
//! * [`paths`] and [`token`]: where Venecita keeps its files and the access token. Always
//!   available, so clients can use them without the server.
//! * With the `server` feature (default): the HTTP/WebSocket server ([`start`],
//!   [`router`]). It knows nothing about windows: it hands every request to the application
//!   as a [`Call`] and sends back the answer. See `docs/API.md`.

pub mod paths;
pub mod token;

#[cfg(feature = "server")]
mod server;
#[cfg(feature = "server")]
pub use server::*;
