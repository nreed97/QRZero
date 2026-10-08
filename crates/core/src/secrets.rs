//! Passwords for online services.
//!
//! On Windows and macOS they live in the OS credential store (Windows
//! Credential Manager / Keychain), never in the log database. On Linux they go
//! to the Secret Service (GNOME Keyring, KWallet) and fall back to the settings
//! table only when no Secret Service is running, as on a headless server. `service` namespaces the entries ("QRZero"
//! normally; tests use their own so they never touch real credentials).

use crate::error::Result;
use crate::store::Store;

#[cfg(any(windows, target_os = "macos"))]
pub fn get(_store: &Store, service: &str, name: &str) -> Result<Option<String>> {
    use crate::error::Error;
    let entry = keyring::Entry::new(service, name).map_err(|e| Error::Secret(e.to_string()))?;
    match entry.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn set(_store: &Store, service: &str, name: &str, value: Option<&str>) -> Result<()> {
    use crate::error::Error;
    let entry = keyring::Entry::new(service, name).map_err(|e| Error::Secret(e.to_string()))?;
    match value {
        Some(v) => entry.set_password(v).map_err(|e| Error::Secret(e.to_string())),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secret(e.to_string())),
        },
    }
}

#[cfg(target_os = "linux")]
pub fn get(store: &Store, service: &str, name: &str) -> Result<Option<String>> {
    use crate::error::Error;
    match keyring::Entry::new(service, name).and_then(|e| e.get_password()) {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => fallback_get(store, service, name),
        Err(keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_)) => fallback_get(store, service, name),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

#[cfg(target_os = "linux")]
pub fn set(store: &Store, service: &str, name: &str, value: Option<&str>) -> Result<()> {
    use crate::error::Error;
    let entry = match keyring::Entry::new(service, name) {
        Ok(e) => e,
        Err(keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_)) => return fallback_set(store, service, name, value),
        Err(e) => return Err(Error::Secret(e.to_string())),
    };
    let done = match value {
        Some(v) => entry.set_password(v),
        None => match entry.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            r => r,
        },
    };
    match done {
        // Stored safely: drop any copy an earlier headless run left in the settings.
        Ok(()) => fallback_set(store, service, name, None),
        Err(keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_)) => fallback_set(store, service, name, value),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn fallback_get(store: &Store, service: &str, name: &str) -> Result<Option<String>> {
    store.get_setting(&format!("secret.{service}.{name}"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn fallback_set(store: &Store, service: &str, name: &str, value: Option<&str>) -> Result<()> {
    let key = format!("secret.{service}.{name}");
    match value {
        Some(v) => store.set_setting(&key, v),
        None => store.delete_setting(&key),
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub use {fallback_get as get, fallback_set as set};
