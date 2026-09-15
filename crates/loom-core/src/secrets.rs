//! API keys live in the OS credential vault, never in config files.
//!
//! Each provider gets one entry under the service `com.ellio.loom`, keyed
//! `provider:<id>`, so rotating or deleting a provider only touches its own
//! secret.

use crate::{Error, Result};

const SERVICE: &str = "com.ellio.loom";

fn entry(provider_id: &str) -> Result<keyring::Entry> {
    if provider_id.trim().is_empty() {
        return Err(Error::Other("provider id must not be empty".into()));
    }
    keyring::Entry::new(SERVICE, &format!("provider:{provider_id}"))
        .map_err(|e| Error::Other(format!("keyring unavailable: {e}")))
}

pub fn set_api_key(provider_id: &str, api_key: &str) -> Result<()> {
    entry(provider_id)?
        .set_password(api_key)
        .map_err(|e| Error::Other(format!("failed to store api key: {e}")))
}

pub fn get_api_key(provider_id: &str) -> Result<Option<String>> {
    match entry(provider_id)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Other(format!("failed to read api key: {e}"))),
    }
}

pub fn delete_api_key(provider_id: &str) -> Result<()> {
    match entry(provider_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Other(format!("failed to delete api key: {e}"))),
    }
}

/// Whether a key exists for this provider without materialising it into the
/// caller (the UI only needs the boolean).
pub fn has_api_key(provider_id: &str) -> bool {
    matches!(get_api_key(provider_id), Ok(Some(_)))
}
