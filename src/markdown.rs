use crate::error::{Error, Result};
use std::fs;

#[derive(Debug, Clone, Default)]
pub struct RepoSyncResult {
    pub name: String,
    pub default_branch: String,
    pub latest_commit_iso: String,
    pub latest_commit_hash: String,
    pub latest_commit_msg: String,
    pub latest_sync_iso: String,
    pub origin_url: String,
    pub target_url: String,
}

pub fn render_table(results: &[RepoSyncResult]) -> String {
    let mut out = String::new();
    out.push_str("| Repository | Default Branch | Latest Commit Time (UTC) | Commit | Message | Last Synced |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for r in results {
        let commit = if r.latest_commit_hash.is_empty() {
            String::new()
        } else {
            let short = if r.latest_commit_hash.len() > 7 {
                &r.latest_commit_hash[..7]
            } else {
                &r.latest_commit_hash
            };
            format!("`{}`", short)
        };
        let mut name = r.name.clone();
        let mut link_url = r.target_url.clone();
        if link_url.is_empty() {
            link_url = r.origin_url.clone();
        }
        if !link_url.is_empty() {
            name = format!("[{}]({})", name, link_url);
        }

        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            escape_pipes(&name),
            escape_pipes(&r.default_branch),
            escape_pipes(&r.latest_commit_iso),
            commit,
            escape_pipes(&r.latest_commit_msg),
            escape_pipes(&r.latest_sync_iso)
        ));
    }
    out
}

pub fn replace_placeholder_in_file(
    path: impl AsRef<std::path::Path>,
    placeholder: &str,
    table: &str,
) -> Result<()> {
    let path_ref = path.as_ref();
    let content = fs::read_to_string(path_ref).map_err(|source| Error::Io {
        path: path_ref.to_path_buf(),
        source,
    })?;
    if !content.contains(placeholder) {
        return Err(Error::PlaceholderNotFound {
            path: path_ref.to_path_buf(),
            placeholder: placeholder.to_string(),
        });
    }
    let new_content = content.replace(placeholder, table);
    fs::write(path_ref, new_content).map_err(|source| Error::Io {
        path: path_ref.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn escape_pipes(s: &str) -> String {
    s.replace('|', "\\|")
}
