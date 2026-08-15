use crate::SecretError;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    /// Stores or replaces a secret.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input or an unavailable credential store.
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), SecretError>;

    /// Reads a secret for use inside the Rust core.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid account or an unavailable credential store.
    fn get_secret(&self, account: &str) -> Result<Option<String>, SecretError>;

    /// Deletes a secret and reports whether it existed.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid account or an unavailable credential store.
    fn delete_secret(&self, account: &str) -> Result<bool, SecretError>;

    /// Reports whether a secret exists without exposing its value to callers.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid account or an unavailable credential store.
    fn has_secret(&self, account: &str) -> Result<bool, SecretError> {
        Ok(self.get_secret(account)?.is_some())
    }
}

pub struct KeyringSecretStore {
    service: String,
    access_lock: Mutex<()>,
}

impl KeyringSecretStore {
    /// Creates a system credential-store adapter scoped to a service name.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::InvalidIdentifier`] for an empty, overlong, or control-character
    /// service identifier.
    pub fn new(service: impl Into<String>) -> Result<Self, SecretError> {
        let service = service.into();
        validate_identifier(&service)?;
        Ok(Self {
            service,
            access_lock: Mutex::new(()),
        })
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretError> {
        validate_identifier(account)?;
        keyring::Entry::new(&self.service, account).map_err(|error| map_keyring_error(&error))
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        if secret.is_empty() {
            return Err(SecretError::InvalidIdentifier);
        }
        let _guard = self
            .access_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        self.entry(account)?
            .set_password(secret)
            .map_err(|error| map_keyring_error(&error))
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>, SecretError> {
        let _guard = self
            .access_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        match self.entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(&error)),
        }
    }

    fn delete_secret(&self, account: &str) -> Result<bool, SecretError> {
        let _guard = self
            .access_lock
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?;
        match self.entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(map_keyring_error(&error)),
        }
    }
}

#[derive(Default)]
pub struct MemorySecretStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        validate_identifier(account)?;
        if secret.is_empty() {
            return Err(SecretError::InvalidIdentifier);
        }
        self.secrets
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>, SecretError> {
        validate_identifier(account)?;
        Ok(self
            .secrets
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .get(account)
            .cloned())
    }

    fn delete_secret(&self, account: &str) -> Result<bool, SecretError> {
        validate_identifier(account)?;
        Ok(self
            .secrets
            .lock()
            .map_err(|_| SecretError::LockPoisoned)?
            .remove(account)
            .is_some())
    }
}

fn validate_identifier(value: &str) -> Result<(), SecretError> {
    let valid =
        !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(SecretError::InvalidIdentifier)
    }
}

fn map_keyring_error(error: &keyring::Error) -> SecretError {
    SecretError::Unavailable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_deletes_secrets() {
        let store = MemorySecretStore::default();
        assert!(!store.has_secret("deepseek").unwrap());
        store
            .set_secret("deepseek", "test-secret")
            .expect("secret should save");
        assert!(store.has_secret("deepseek").unwrap());
        assert_eq!(
            store.get_secret("deepseek").unwrap().as_deref(),
            Some("test-secret")
        );
        assert!(store.delete_secret("deepseek").unwrap());
        assert!(!store.delete_secret("deepseek").unwrap());
    }

    #[test]
    fn invalid_identifiers_and_empty_secrets_are_rejected() {
        let store = MemorySecretStore::default();
        assert!(matches!(
            store.set_secret("\n", "secret"),
            Err(SecretError::InvalidIdentifier)
        ));
        assert!(matches!(
            store.set_secret("provider", ""),
            Err(SecretError::InvalidIdentifier)
        ));
    }
}
