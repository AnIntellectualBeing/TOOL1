//! PhoneMirror's platform-independent logic.
//!
//! Nothing in this crate touches the network, spawns processes or opens
//! windows directly: those go through traits ([`adb::CommandRunner`],
//! [`discovery::Discovery`]) that the desktop app implements and the tests
//! mock. That keeps pairing, discovery and engine arguments unit-testable on
//! any OS.

pub mod adb;
pub mod discovery;
pub mod errors;
pub mod firewall;
pub mod net;
pub mod pairing;
pub mod qr;
pub mod scrcpy;
pub mod settings;
pub mod store;
pub mod uxplay;

pub use errors::Problem;
