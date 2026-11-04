use keyring::Entry;
use url::Url;

use crate::{Credentials, ProxyAuthError, SecretHeader, server_origin};

pub trait CredentialVault {
    fn load(&self, url: &Url) -> Result<Credentials, ProxyAuthError>;
    fn save(&self, url: &Url, credentials: &Credentials) -> Result<(), ProxyAuthError>;
    fn delete(&self, url: &Url) -> Result<(), ProxyAuthError>;
}

pub struct OsCredentialVault;

#[derive(Default)]
pub struct PersistenceState {
    pub remembered_names: Option<Vec<String>>,
    pub cleanup_pending: bool,
}

/// Cleanup failures must not prevent recovery. The caller persists the tombstone and reports it.
pub fn commit_credentials(
    vault: &impl CredentialVault,
    store: &crate::AuthStore,
    url: &Url,
    credentials: Option<Credentials>,
    remember: bool,
    state: &mut PersistenceState,
) -> Result<(), ProxyAuthError> {
    if credentials
        .as_ref()
        .is_some_and(|c| server_origin(url).ok().as_ref() != Some(&c.origin))
    {
        return Err(ProxyAuthError::InvalidOrigin);
    }
    if let Some(credentials) = credentials.as_ref().filter(|_| remember) {
        vault.save(url, credentials)?;
        state.remembered_names = Some(credentials.names());
        state.cleanup_pending = false;
    } else {
        if state.remembered_names.is_some() || state.cleanup_pending {
            state.cleanup_pending = vault.delete(url).is_err();
        }
        state.remembered_names = None;
    }
    match credentials {
        Some(credentials) => store.activate(url, Ok(credentials))?,
        None => store.clear(),
    }
    Ok(())
}

fn entry(url: &Url) -> Result<Entry, ProxyAuthError> {
    let service = if cfg!(debug_assertions) {
        "drop-debug.reverse-proxy"
    } else {
        "drop.reverse-proxy"
    };
    Entry::new(service, &server_origin(url)?).map_err(|_| ProxyAuthError::StorageUnavailable)
}

impl CredentialVault for OsCredentialVault {
    fn load(&self, url: &Url) -> Result<Credentials, ProxyAuthError> {
        load_entry(url, &entry(url)?)
    }
    fn save(&self, url: &Url, credentials: &Credentials) -> Result<(), ProxyAuthError> {
        if server_origin(url)? != credentials.origin {
            return Err(ProxyAuthError::InvalidOrigin);
        }
        let secret = serde_json::to_string(credentials.secrets())
            .map_err(|_| ProxyAuthError::StorageUnavailable)?;
        entry(url)?
            .set_password(&secret)
            .map_err(|_| ProxyAuthError::StorageUnavailable)
    }
    fn delete(&self, url: &Url) -> Result<(), ProxyAuthError> {
        match entry(url)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(ProxyAuthError::StorageUnavailable),
        }
    }
}

fn load_entry(url: &Url, entry: &Entry) -> Result<Credentials, ProxyAuthError> {
    let secret = entry.get_password().map_err(|error| match error {
        keyring::Error::NoEntry => ProxyAuthError::MissingCredentials,
        _ => ProxyAuthError::StorageUnavailable,
    })?;
    let headers: Vec<SecretHeader> =
        serde_json::from_str(&secret).map_err(|_| ProxyAuthError::MissingCredentials)?;
    Credentials::new(url, headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyring::mock::MockCredential;
    use std::cell::Cell;

    struct LockedVault {
        locked: Cell<bool>,
        deleted: Cell<bool>,
    }
    impl CredentialVault for LockedVault {
        fn load(&self, _: &Url) -> Result<Credentials, ProxyAuthError> {
            Err(ProxyAuthError::StorageUnavailable)
        }
        fn save(&self, _: &Url, _: &Credentials) -> Result<(), ProxyAuthError> {
            if self.locked.get() {
                Err(ProxyAuthError::StorageUnavailable)
            } else {
                Ok(())
            }
        }
        fn delete(&self, _: &Url) -> Result<(), ProxyAuthError> {
            if self.locked.get() {
                Err(ProxyAuthError::StorageUnavailable)
            } else {
                self.deleted.set(true);
                Ok(())
            }
        }
    }

    #[test]
    fn locked_vault_allows_session_replacement_and_disabling_with_explicit_cleanup_state() {
        let url = Url::parse("https://drop.example").unwrap();
        let vault = LockedVault {
            locked: Cell::new(true),
            deleted: Cell::new(false),
        };
        let store = crate::AuthStore::default();
        store
            .activate(&url, Err(ProxyAuthError::StorageUnavailable))
            .unwrap();
        let mut state = PersistenceState {
            remembered_names: Some(vec!["X-Token".into()]),
            cleanup_pending: false,
        };
        let fresh = Credentials::new(
            &url,
            vec![SecretHeader {
                name: "X-Token".into(),
                value: "fresh-session-token".into(),
            }],
        )
        .unwrap();
        commit_credentials(&vault, &store, &url, Some(fresh), false, &mut state).unwrap();
        assert_eq!(
            store.headers_for(&url).unwrap().unwrap()["x-token"],
            "fresh-session-token"
        );
        assert!(state.remembered_names.is_none());
        assert!(state.cleanup_pending);
        commit_credentials(&vault, &store, &url, None, false, &mut state).unwrap();
        assert!(store.headers_for(&url).unwrap().is_none());
        assert!(state.cleanup_pending);
        vault.locked.set(false);
        commit_credentials(&vault, &store, &url, None, false, &mut state).unwrap();
        assert!(vault.deleted.get());
        assert!(!state.cleanup_pending);
        assert!(state.remembered_names.is_none());
    }

    #[test]
    fn failed_remember_preserves_session_and_overwriting_old_record_cancels_cleanup() {
        let url = Url::parse("https://drop.example").unwrap();
        let vault = LockedVault {
            locked: Cell::new(true),
            deleted: Cell::new(false),
        };
        let store = crate::AuthStore::default();
        let fresh = Credentials::new(
            &url,
            vec![SecretHeader {
                name: "X-Token".into(),
                value: "session-token".into(),
            }],
        )
        .unwrap();
        store.activate(&url, Ok(fresh.clone())).unwrap();
        let mut state = PersistenceState {
            remembered_names: None,
            cleanup_pending: true,
        };
        assert!(
            commit_credentials(&vault, &store, &url, Some(fresh.clone()), true, &mut state)
                .is_err()
        );
        assert_eq!(
            store.headers_for(&url).unwrap().unwrap()["x-token"],
            "session-token"
        );
        assert!(state.cleanup_pending);
        vault.locked.set(false);
        commit_credentials(&vault, &store, &url, Some(fresh), true, &mut state).unwrap();
        assert!(!state.cleanup_pending);
        assert!(state.remembered_names.is_some());
        assert!(!vault.deleted.get());
    }

    #[test]
    fn credential_store_loads_valid_records_and_rejects_missing_or_corrupt_records() {
        let entry = Entry::new_with_credential(Box::new(MockCredential::default()));
        let url = Url::parse("https://drop.example").unwrap();
        assert_eq!(
            load_entry(&url, &entry).err(),
            Some(ProxyAuthError::MissingCredentials)
        );
        entry
            .set_password(r#"[{"name":"X-Token","value":"private-token"}]"#)
            .unwrap();
        assert_eq!(
            load_entry(&url, &entry).unwrap().secrets()[0].value,
            "private-token"
        );
        entry.set_password("corrupted private-token").unwrap();
        let error = load_entry(&url, &entry).err().unwrap();
        assert_eq!(error, ProxyAuthError::MissingCredentials);
        assert!(!format!("{error:?}: {error}").contains("private-token"));
        entry.delete_credential().unwrap();
        assert_eq!(
            load_entry(&url, &entry).err(),
            Some(ProxyAuthError::MissingCredentials)
        );
    }

    #[test]
    fn credential_store_failures_never_expose_backend_diagnostics() {
        let entry = Entry::new_with_credential(Box::new(MockCredential::default()));
        let mock: &MockCredential = entry.get_credential().downcast_ref().unwrap();
        mock.set_error(keyring::Error::PlatformFailure(Box::new(
            std::io::Error::other("private-token"),
        )));
        let error = load_entry(&Url::parse("https://drop.example").unwrap(), &entry)
            .err()
            .unwrap();
        assert_eq!(error, ProxyAuthError::StorageUnavailable);
        assert!(!format!("{error:?}: {error}").contains("private-token"));
    }
}
