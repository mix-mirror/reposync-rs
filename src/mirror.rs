use crate::{
    config::{Auth, AuthType, DEFAULT_REFSPECS, Repo},
    error::{Error, Result},
    markdown::RepoSyncResult,
};
use chrono::{DateTime, SecondsFormat, Utc};
use git2::{
    Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, RepositoryInitOptions,
    Time as GitTime,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime},
};
use tracing::{debug, info};

static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_progress_enabled(enabled: bool) {
    PROGRESS_ENABLED.store(enabled, Ordering::Relaxed);
}

#[derive(Debug, Clone)]
pub struct HeadInfo {
    name: String,
    hash: String,
    when: DateTime<Utc>,
    msg: String,
}

pub fn clone_mirror_and_inspect(repo: &Repo, refspecs: &[String]) -> Result<RepoSyncResult> {
    let start = SystemTime::now();
    let workdir = create_temp_dir()?;
    let _cleanup = TempDir {
        path: workdir.clone(),
    };

    let effective_refspecs: Vec<String> = if refspecs.is_empty() {
        DEFAULT_REFSPECS.iter().map(|s| s.to_string()).collect()
    } else {
        refspecs.to_vec()
    };
    let refspecs_borrowed: Vec<&str> = effective_refspecs.iter().map(|s| s.as_str()).collect();

    let bare_path = workdir.join("bare.git");
    let mut init_opts = RepositoryInitOptions::new();
    init_opts.bare(true);
    let r = Repository::init_opts(&bare_path, &init_opts).map_err(|source| Error::Git {
        context: "init bare",
        source,
    })?;
    debug!(repo = %repo.display_name(), "initialized bare repo");

    // origin remote
    r.remote("origin", &repo.origin.url)
        .map_err(|source| Error::Git {
            context: "create origin remote",
            source,
        })?;

    let mut fetch_cb = RemoteCallbacks::new();
    {
        let auth = repo.origin.auth.clone();
        fetch_cb.credentials(move |_, username_from_url, allowed| {
            build_credentials(&auth, username_from_url, allowed)
        });
    }
    if PROGRESS_ENABLED.load(Ordering::Relaxed) {
        fetch_cb.transfer_progress(|stats| {
            if stats.total_objects() > 0 {
                let pct = (100 * stats.received_objects()) / stats.total_objects();
                info!(progress = %pct, received = %stats.received_objects(), total = %stats.total_objects(), "git fetch progress");
            }
            true
        });
    }
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(fetch_cb);

    {
        let mut remote = r.find_remote("origin").map_err(|source| Error::Git {
            context: "find origin remote",
            source,
        })?;
        remote
            .fetch(&refspecs_borrowed, Some(&mut fetch_opts), None)
            .map_err(|source| Error::Git {
                context: "fetch origin",
                source,
            })?;
    }
    debug!(repo = %repo.display_name(), "fetched from origin");

    let HeadInfo {
        name: default_branch,
        hash: latest_hash,
        msg: latest_msg,
        when: latest_time,
    } = resolve_default_and_latest(&r)?;

    // target remote
    r.remote("target", &repo.target.url)
        .map_err(|source| Error::Git {
            context: "create target remote",
            source,
        })?;

    let mut push_cb = RemoteCallbacks::new();
    {
        let auth = repo.target.auth.clone();
        push_cb.credentials(move |_, username_from_url, allowed| {
            build_credentials(&auth, username_from_url, allowed)
        });
    }
    if PROGRESS_ENABLED.load(Ordering::Relaxed) {
        push_cb.transfer_progress(|stats| {
            if stats.total_objects() > 0 {
                let pct = (100 * stats.received_objects()) / stats.total_objects();
                info!(progress = %pct, received = %stats.received_objects(), total = %stats.total_objects(), "git push progress");
            }
            true
        });
    }
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(push_cb);

    {
        let mut remote = r.find_remote("target").map_err(|source| Error::Git {
            context: "find target remote",
            source,
        })?;
        remote
            .push(&refspecs_borrowed, Some(&mut push_opts))
            .map_err(|source| Error::Git {
                context: "push target",
                source,
            })?;
    }

    info!(
        repo = %repo.display_name(),
        elapsed_ms = %duration_ms(start.elapsed().ok()),
        "mirror complete"
    );

    Ok(RepoSyncResult {
        name: repo.display_name(),
        default_branch,
        latest_commit_iso: latest_time.to_rfc3339_opts(SecondsFormat::Secs, true),
        latest_commit_hash: latest_hash,
        latest_commit_msg: latest_msg,
        latest_sync_iso: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        origin_url: repo.origin.url.clone(),
        target_url: repo.target.url.clone(),
    })
}

fn resolve_default_and_latest(r: &Repository) -> Result<HeadInfo> {
    let mut heads: Vec<HeadInfo> = Vec::new();
    if let Ok(refs) = r.references() {
        for reference in refs.flatten() {
            if !reference.is_branch() {
                continue;
            }
            if let Some(target) = reference.target()
                && let Ok(commit) = r.find_commit(target)
            {
                let when = git_time_to_utc(commit.time());
                heads.push(HeadInfo {
                    name: reference.shorthand().unwrap_or_default().to_string(),
                    hash: commit.id().to_string(),
                    when,
                    msg: first_line(commit.message().unwrap_or_default()),
                });
            }
        }
    }

    if heads.is_empty() {
        return Ok(HeadInfo {
            name: "main".to_string(),
            hash: String::new(),
            when: Utc::now(),
            msg: String::new(),
        });
    }

    for candidate in ["main", "master"] {
        if let Some(h) = heads.iter().find(|h| h.name == candidate) {
            return Ok(h.clone());
        }
    }

    heads.sort_by(|a, b| b.when.cmp(&a.when));
    match heads.first() {
        Some(h) => Ok(h.clone()),
        None => Err(Error::Git {
            context: "resolve default and latest",
            source: git2::Error::from_str("no heads found"),
        }),
    }
}

fn first_line(s: &str) -> String {
    match s.split_once('\n') {
        Some((first, _)) => first.to_string(),
        None => s.to_string(),
    }
}

fn git_time_to_utc(t: GitTime) -> DateTime<Utc> {
    let secs = t.seconds();
    DateTime::<Utc>::from_timestamp(secs, 0).unwrap_or_else(Utc::now)
}

fn duration_ms(dur: Option<Duration>) -> u128 {
    dur.map(|d| d.as_millis()).unwrap_or(0)
}

fn create_temp_dir() -> Result<PathBuf> {
    let mut base = std::env::temp_dir();
    let unique = format!(
        "reposync-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    base.push(unique);
    fs::create_dir_all(&base).map_err(|source| Error::Io {
        path: base.clone(),
        source,
    })?;
    Ok(base)
}

struct TempDir {
    path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.path) {
            debug!("failed to remove temp dir {}: {}", self.path.display(), err);
        }
    }
}

fn build_credentials(
    auth: &Auth,
    username_from_url: Option<&str>,
    allowed: git2::CredentialType,
) -> Result<Cred, git2::Error> {
    let _ = allowed;
    match &auth.r#type {
        AuthType::None => Ok(Cred::default()?),
        AuthType::Http => {
            if !auth.token.is_empty() {
                return Cred::userpass_plaintext("token", &auth.token);
            }
            Ok(Cred::userpass_plaintext(&auth.username, &auth.password)?)
        }
        AuthType::Ssh => {
            let username = username_from_url.unwrap_or("git");
            if !auth.ssh_private_key.is_empty() {
                return Cred::ssh_key_from_memory(
                    username,
                    None,
                    &auth.ssh_private_key,
                    Some(&auth.ssh_passphrase),
                );
            }
            let key_path = if !auth.ssh_private_key_path.is_empty() {
                PathBuf::from(&auth.ssh_private_key_path)
            } else {
                default_ssh_key_path()
            };
            Ok(Cred::ssh_key(
                username,
                None,
                &key_path,
                if auth.ssh_passphrase.is_empty() {
                    None
                } else {
                    Some(auth.ssh_passphrase.as_str())
                },
            )?)
        }
    }
}

fn default_ssh_key_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    for name in ["id_ed25519", "id_rsa"] {
        let candidate = Path::new(&home).join(".ssh").join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    Path::new(&home).join(".ssh").join("id_ed25519")
}
