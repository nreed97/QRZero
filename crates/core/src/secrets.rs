//! Passwords for online services.
//!
//! On Windows and macOS they live in the OS credential store (Windows
//! Credential Manager / Keychain), never in the log database. Elsewhere they
//! fall back to the settings table.

use crate::error::Result;
use crate::store::Store;

const SERVICE: &str = "QRZero";

#[cfg(any(windows, target_os = "macos"))]
pub fn get(_store: &Store, name: &str) -> Result<Option<String>> {
    use crate::error::Error;
    let entry = keyring::Entry::new(SERVICE, name).map_err(|e| Error::Secret(e.to_string()))?;
    match entry.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Secret(e.to_string())),
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn set(_store: &Store, name: &str, value: Option<&str>) -> Result<()> {
    use crate::error::Error;
    let entry = keyring::Entry::new(SERVICE, name).map_err(|e| Error::Secret(e.to_string()))?;
    match value {
        Some(v) => entry.set_password(v).map_err(|e| Error::Secret(e.to_string())),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secret(e.to_string())),
        },
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get(store: &Store, name: &str) -> Result<Option<String>> {
    store.get_setting(&format!("secret.{SERVICE}.{name}"))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn set(store: &Store, name: &str, value: Option<&str>) -> Result<()> {
    let key = format!("secret.{SERVICE}.{name}");
    match value {
        Some(v) => store.set_setting(&key, v),
        None => store.delete_setting(&key),
    }
}
