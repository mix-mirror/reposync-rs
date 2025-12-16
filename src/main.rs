mod config;
mod error;
mod markdown;
mod mirror;

use crate::error::{Error, Result};
use clap::Parser;
use mimalloc::MiMalloc;
use rayon::prelude::*;
use std::{cmp::Ordering as CmpOrdering, path::PathBuf};
use tracing::{error, info};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Debug, Parser)]
#[command(
    name = "reposync-rs",
    about = "Mirror git repositories and produce markdown summaries."
)]
struct Cli {
    #[arg(long, help = "Path to YAML config file")]
    config: PathBuf,

    #[arg(long, help = "Path to markdown file to update (optional)")]
    markdown: Option<PathBuf>,

    #[arg(
        long,
        default_value = "<!-- REPOSYNC -->",
        help = "Placeholder token to replace in markdown"
    )]
    placeholder: String,

    #[arg(
        long,
        default_value_t = default_concurrency(),
        help = "Max concurrent repo syncs"
    )]
    concurrency: usize,

    #[arg(
        long,
        default_value_t = true,
        help = "Stream git fetch/push progress logs"
    )]
    progress: bool,
}

fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let config = config::load_config(&cli.config)?;
    if config.repos.is_empty() {
        return Err(Error::EmptyConfig);
    }

    let global_refspecs = if config.refspecs.is_empty() {
        config::DEFAULT_REFSPECS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        config.refspecs.clone()
    };

    mirror::set_progress_enabled(cli.progress);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cli.concurrency.max(1))
        .build()
        .map_err(|source| Error::ThreadPool { source })?;

    let pairs = pool.install(|| {
        config
            .repos
            .par_iter()
            .enumerate()
            .map(|(idx, repo)| {
                let refspecs = repo.effective_refspecs(&global_refspecs);
                let start = std::time::Instant::now();
                match mirror::clone_mirror_and_inspect(repo, &refspecs) {
                    Ok(res) => {
                        info!(
                            repo = %repo.display_name(),
                            elapsed_ms = %start.elapsed().as_millis(),
                            "sync complete"
                        );
                        (idx, res)
                    }
                    Err(err) => {
                        error!(
                            repo = %repo.display_name(),
                            error = %format!("{err}"),
                            "sync error"
                        );
                        (
                            idx,
                            markdown::RepoSyncResult {
                                name: repo.display_name(),
                                default_branch: String::new(),
                                latest_commit_iso: String::new(),
                                latest_commit_hash: String::new(),
                                latest_commit_msg: format!("ERROR: {err}"),
                                latest_sync_iso: chrono::Utc::now()
                                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                                origin_url: repo.origin.url.clone(),
                                target_url: repo.target.url.clone(),
                            },
                        )
                    }
                }
            })
            .collect::<Vec<_>>()
    });

    let mut results = vec![markdown::RepoSyncResult::default(); pairs.len()];
    for (idx, res) in pairs {
        if idx < results.len() {
            results[idx] = res;
        }
    }

    sort_results(&mut results);

    let table = markdown::render_table(&results);
    if let Some(path) = cli.markdown {
        markdown::replace_placeholder_in_file(path, &cli.placeholder, &table)?;
        info!("markdown updated");
    } else {
        println!("{table}");
    }

    Ok(())
}

fn sort_results(results: &mut [markdown::RepoSyncResult]) {
    results.sort_by(|a, b| {
        match (
            a.latest_commit_iso.is_empty(),
            b.latest_commit_iso.is_empty(),
        ) {
            (true, true) => a.name.cmp(&b.name),
            (true, false) => CmpOrdering::Greater,
            (false, true) => CmpOrdering::Less,
            (false, false) => {
                let ta = chrono::DateTime::parse_from_rfc3339(&a.latest_commit_iso).ok();
                let tb = chrono::DateTime::parse_from_rfc3339(&b.latest_commit_iso).ok();
                match (ta, tb) {
                    (Some(ta), Some(tb)) => tb.cmp(&ta),
                    (Some(_), None) => CmpOrdering::Less,
                    (None, Some(_)) => CmpOrdering::Greater,
                    (None, None) => a.name.cmp(&b.name),
                }
            }
        }
    });
}
