//! Bibliothèque commune à la ligne de commande (`omnictl`) et à l'interface (`omnictl-gui`).

#[cfg(windows)]
#[path = "hid_windows.rs"]
pub mod hid;
#[cfg(target_os = "linux")]
#[path = "hid_linux.rs"]
pub mod hid;
pub mod json;
pub mod keyfx;
pub mod keys;
pub mod link;
pub mod mcp;
pub mod mouse;
pub mod proto;
pub mod store;
