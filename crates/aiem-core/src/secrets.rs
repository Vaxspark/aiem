//! Secret vault — stores secret values in the OS keyring (Windows Credential
//! Manager / macOS Keychain / Linux Secret Service), with a plaintext index
//! kept in `~/.aiem/secrets.json` so we can enumerate names without unlocking.
//!
//! MCP `env` / `headers` values of the form `${secret:NAME}` are resolved by
//! [`expand`] at sync time.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::fs_util::{atomic_write, strip_utf8_bom};
use crate::{paths, Error, Result};

const SERVICE: &str = "aiem";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretMeta {
    #[serde(default)]
    pub description: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SecretIndex {
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretMeta>,
}

pub struct Vault {
    index: SecretIndex,
}

impl Vault {
    pub fn file() -> Result<PathBuf> {
        paths::secrets_index_file()
    }

    pub fn load() -> Result<Self> {
        let p = Self::file()?;
        if !p.exists() {
            return Ok(Self {
                index: SecretIndex::default(),
            });
        }
        let bytes = std::fs::read(&p)?;
        let index: SecretIndex = serde_json::from_slice(strip_utf8_bom(&bytes))?;
        Ok(Self { index })
    }

    pub fn save(&self) -> Result<()> {
        paths::ensure_layout()?;
        let data = serde_json::to_vec_pretty(&self.index)?;
        atomic_write(&Self::file()?, &data)?;
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.index.secrets.keys()
    }
    pub fn meta(&self, name: &str) -> Option<&SecretMeta> {
        self.index.secrets.get(name)
    }
    pub fn len(&self) -> usize {
        self.index.secrets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.secrets.is_empty()
    }

    /// Store a secret value in the OS keyring; persist name → meta in the index.
    pub fn set(&mut self, name: &str, value: &str, description: Option<String>) -> Result<()> {
        validate_name(name)?;
        let entry =
            keyring::Entry::new(SERVICE, name).map_err(|e| Error::Keyring(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| Error::Keyring(e.to_string()))?;
        self.index.secrets.insert(
            name.to_string(),
            SecretMeta {
                description,
                updated_at: Utc::now(),
            },
        );
        self.save()?;
        Ok(())
    }

    /// Fetch a secret value from the OS keyring. Returns `NotFound` if missing.
    pub fn get(&self, name: &str) -> Result<String> {
        let entry =
            keyring::Entry::new(SERVICE, name).map_err(|e| Error::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(v) => Ok(v),
            Err(keyring::Error::NoEntry) => {
                Err(Error::NotFound(format!("secret `{name}` not in keyring")))
            }
            Err(e) => Err(Error::Keyring(e.to_string())),
        }
    }

    /// Delete a secret from both the keyring and the index.
    pub fn delete(&mut self, name: &str) -> Result<()> {
        let entry =
            keyring::Entry::new(SERVICE, name).map_err(|e| Error::Keyring(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(Error::Keyring(e.to_string())),
        }
        self.index.secrets.remove(name);
        self.save()?;
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::Invalid("secret name cannot be empty".into()));
    }
    if name.chars().any(|c| c.is_whitespace()) {
        return Err(Error::Invalid(
            "secret name cannot contain whitespace".into(),
        ));
    }
    Ok(())
}

/// Expand `${secret:NAME}` placeholders in `s` using the OS keyring. Missing
/// secrets are left as-is (the caller decides whether to warn / fail).
pub fn expand(s: &str) -> String {
    expand_with(s, |name| {
        let entry = keyring::Entry::new(SERVICE, name).ok()?;
        entry.get_password().ok()
    })
}

/// Version that takes a closure so tests / offline paths can inject values.
pub fn expand_with<F: FnMut(&str) -> Option<String>>(s: &str, mut lookup: F) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find('}') {
            let inner = &after[..end];
            if let Some(name) = inner.strip_prefix("secret:") {
                if let Some(v) = lookup(name) {
                    out.push_str(&v);
                    rest = &after[end + 1..];
                    continue;
                }
            }
            // Not a secret placeholder or unknown — keep literal.
            out.push_str("${");
            out.push_str(inner);
            out.push('}');
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expand_basic() {
        let got = expand_with("Bearer ${secret:token} plain", |n| {
            if n == "token" {
                Some("abc".into())
            } else {
                None
            }
        });
        assert_eq!(got, "Bearer abc plain");
    }
    #[test]
    fn expand_missing_preserves() {
        let got = expand_with("${secret:missing}", |_| None);
        assert_eq!(got, "${secret:missing}");
    }
}
