use crate::error::{Error, Result};
use base64::Engine;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_yaml::Value;
use std::{
    env::VarError,
    fs,
    path::{Path, PathBuf},
};
use tracing::warn;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<Repo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Repo {
    #[serde(default)]
    pub name: String,
    pub origin: Remote,
    pub target: Remote,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Remote {
    pub url: String,
    #[serde(default)]
    pub auth: Auth,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Auth {
    #[serde(default)]
    pub r#type: AuthType,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub ssh_private_key_path: String,
    #[serde(default)]
    pub ssh_private_key: String,
    #[serde(default)]
    pub ssh_passphrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AuthType {
    #[default]
    None,
    Http,
    Ssh,
}

impl<'de> Deserialize<'de> for AuthType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim();
        let lower = trimmed.to_ascii_lowercase();
        let variant = match lower.as_str() {
            "" | "none" => AuthType::None,
            "http" => AuthType::Http,
            "ssh" => AuthType::Ssh,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "unknown auth type: {}",
                    trimmed
                )));
            }
        };
        Ok(variant)
    }
}

impl Repo {
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        let url = self.origin.url.trim();
        if url.is_empty() {
            return "<unnamed>".to_string();
        }
        if url.contains("://") {
            if let Some(seg) = url.split('/').rfind(|s| !s.is_empty()) {
                return seg.trim_end_matches(".git").to_string();
            }
        } else if let Some((_, rest)) = url.split_once(':')
            && let Some(seg) = rest.split('/').rfind(|s| !s.is_empty())
        {
            return seg.trim_end_matches(".git").to_string();
        }
        url.to_string()
    }
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config> {
    let pathbuf = path.as_ref().to_path_buf();
    let raw = fs::read_to_string(&pathbuf).map_err(|source| Error::Io {
        path: pathbuf.clone(),
        source,
    })?;

    let mut value: Value = serde_yaml::from_str(&raw).map_err(|source| Error::Yaml {
        path: pathbuf.clone(),
        source,
    })?;
    expand_value(&mut value)?;

    let cfg: Config = serde_yaml::from_value(value).map_err(|source| Error::Yaml {
        path: pathbuf,
        source,
    })?;
    Ok(cfg)
}

fn expand_value(v: &mut Value) -> Result<()> {
    match v {
        Value::String(s) => {
            let expanded = expand_and_resolve(s)?;
            if expanded != *s {
                *s = expanded;
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                expand_value(item)?;
            }
        }
        Value::Mapping(map) => {
            for (_, val) in map {
                expand_value(val)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn expand_and_resolve(s: &str) -> Result<String> {
    let expanded = expand_env(s);
    resolve_secret(&expanded)
}

fn expand_env(s: &str) -> String {
    // Replace $VAR and ${VAR} using regex for robustness
    let re = Regex::new(r"\$\{([A-Za-z0-9_]+)\}|\$([A-Za-z0-9_]+)").unwrap();
    re.replace_all(s, |caps: &Captures| {
        let key = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or_default();
        resolve_env(key)
    })
    .into_owned()
}

fn resolve_secret(s: &str) -> Result<String> {
    match s.trim().split_once(':') {
        Some(("env", val)) => Ok(resolve_env(val)),
        Some(("env-b64", val)) => {
            let env_val = resolve_env(val);
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(env_val)
                .map_err(|source| Error::SecretDecode {
                    key: val.to_string(),
                    source,
                })?;
            Ok(String::from_utf8_lossy(&decoded).to_string())
        }
        Some(("file", val)) => {
            let content = fs::read_to_string(val).map_err(|source| Error::SecretFile {
                path: PathBuf::from(val),
                source,
            })?;
            Ok(content)
        }
        Some(("file-b64", val)) => {
            let bytes = fs::read(val).map_err(|source| Error::SecretFile {
                path: PathBuf::from(val),
                source,
            })?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(bytes)
                .map_err(|source| Error::SecretDecode {
                    key: val.to_string(),
                    source,
                })?;
            Ok(String::from_utf8_lossy(&decoded).to_string())
        }
        _ => Ok(s.to_string()),
    }
}

fn resolve_env(key: &str) -> String {
    match std::env::var(key) {
        Ok(env_val) => env_val,
        Err(VarError::NotPresent) => {
            warn!(name = key, "environment variable not present");
            String::new()
        }
        Err(VarError::NotUnicode(_)) => {
            warn!(
                name = key,
                "environment variable not a valid unicode string"
            );
            String::new()
        }
    }
}
