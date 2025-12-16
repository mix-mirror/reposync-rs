use snafu::Snafu;
use std::{path::PathBuf, result};

pub type Result<T, E = Error> = result::Result<T, E>;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("IO error at {}: {source}", path.display()))]
    Io {
        path: PathBuf,
        #[snafu(source)]
        source: std::io::Error,
    },

    #[snafu(display("Failed to parse YAML at {}: {source}", path.display()))]
    Yaml {
        path: PathBuf,
        #[snafu(source)]
        source: serde_yaml::Error,
    },

    #[snafu(display("Git error during {}: {source}", context))]
    Git {
        context: &'static str,
        #[snafu(source)]
        source: git2::Error,
    },

    #[snafu(display("Failed to decode base64 secret for {}", key))]
    SecretDecode {
        key: String,
        #[snafu(source)]
        source: base64::DecodeError,
    },

    #[snafu(display("Failed to read secret file {}", path.display()))]
    SecretFile {
        path: PathBuf,
        #[snafu(source)]
        source: std::io::Error,
    },

    #[snafu(display("Unknown auth type: {}", auth_type))]
    UnknownAuthType { auth_type: String },

    #[snafu(display("Placeholder not found in {}: {}", path.display(), placeholder))]
    PlaceholderNotFound { path: PathBuf, placeholder: String },

    #[snafu(display("No repositories configured"))]
    EmptyConfig,

    #[snafu(display("Failed to build thread pool: {source}"))]
    ThreadPool {
        #[snafu(source)]
        source: rayon::ThreadPoolBuildError,
    },
}
