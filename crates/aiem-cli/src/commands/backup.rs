use std::path::PathBuf;

use clap::Subcommand;

use aiem_core::backup;

#[derive(Subcommand, Debug)]
pub enum BackupCmd {
    /// Take a local timestamped snapshot (~/.aiem/snapshots/<ts>/).
    Snapshot,

    /// Export config files to an explicit directory.
    Export {
        /// Destination directory (will be created if it does not exist).
        dest: PathBuf,
    },

    /// Restore config files from a snapshot or export directory.
    Import {
        /// Source directory produced by `snapshot` or `export`.
        src: PathBuf,
    },

    /// Commit and push backup to a GitHub repository.
    Push {
        /// HTTPS URL of the backup repo, e.g. `https://github.com/you/aiem-backup`.
        /// If omitted, uses the URL saved from the previous push.
        #[arg(long)]
        repo: Option<String>,

        /// GitHub PAT (optional; falls back to GITHUB_TOKEN env var).
        #[arg(long)]
        token: Option<String>,
    },

    /// Pull and restore backup from a GitHub repository.
    Pull {
        /// HTTPS URL of the backup repo.
        /// If omitted, uses the URL saved from the previous push.
        #[arg(long)]
        repo: Option<String>,

        /// GitHub PAT (optional; falls back to GITHUB_TOKEN env var).
        #[arg(long)]
        token: Option<String>,
    },

    /// Show backup configuration (auto-interval, last backup time, repo URL).
    Status,

    /// Set the auto-backup interval.
    SetInterval {
        /// `never`, `daily`, or `weekly`.
        interval: IntervalArg,
    },

    /// List existing local snapshots.
    List,

    /// Export a self-contained zip (config files + all custom/local skills).
    ExportZip {
        /// Destination path for the zip file (e.g. `~/my-backup.zip`).
        dest: PathBuf,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum IntervalArg {
    Never,
    Daily,
    Weekly,
}

impl From<IntervalArg> for backup::AutoInterval {
    fn from(a: IntervalArg) -> Self {
        match a {
            IntervalArg::Never => backup::AutoInterval::Never,
            IntervalArg::Daily => backup::AutoInterval::Daily,
            IntervalArg::Weekly => backup::AutoInterval::Weekly,
        }
    }
}

pub fn run(cmd: BackupCmd) -> anyhow::Result<()> {
    match cmd {
        BackupCmd::Snapshot => {
            let path = backup::snapshot_local()?;
            println!("snapshot saved: {}", path.display());
        }

        BackupCmd::Export { dest } => {
            let files = backup::export_to_dir(&dest)?;
            println!("exported {} file(s) to {}", files.len(), dest.display());
            for f in &files {
                println!("  {}", f.display());
            }
        }

        BackupCmd::Import { src } => {
            let files = backup::import_from_dir(&src)?;
            println!("restored {} file(s):", files.len());
            for f in &files {
                println!("  {}", f.display());
            }
        }

        BackupCmd::Push { repo, token } => {
            let repo_url = resolve_repo(repo)?;
            backup::push_github(&repo_url, token.as_deref())?;
            println!("pushed to {repo_url}");
        }

        BackupCmd::Pull { repo, token } => {
            let repo_url = resolve_repo(repo)?;
            backup::pull_github(&repo_url, token.as_deref())?;
            println!("restored from {repo_url}");
        }

        BackupCmd::ExportZip { dest } => {
            backup::export_zip(&dest)?;
            println!("zip exported to {}", dest.display());
        }

        BackupCmd::Status => {
            let cfg = backup::BackupConfig::load()?;
            println!("auto-interval : {}", cfg.auto_interval.label());
            println!(
                "github repo   : {}",
                cfg.github_repo.as_deref().unwrap_or("(not set)")
            );
            let last = cfg
                .last_backup_ts
                .map(backup::time_ago)
                .unwrap_or_else(|| "never".into());
            println!("last backup   : {last}");
            println!("due now       : {}", cfg.is_due());
        }

        BackupCmd::SetInterval { interval } => {
            let mut cfg = backup::BackupConfig::load()?;
            cfg.auto_interval = interval.into();
            cfg.save()?;
            println!("auto-interval updated to {}", cfg.auto_interval.label());
        }

        BackupCmd::List => {
            let snaps = backup::list_snapshots()?;
            if snaps.is_empty() {
                println!("no snapshots found");
            } else {
                for s in &snaps {
                    println!("{}", s.display());
                }
            }
        }
    }
    Ok(())
}

fn resolve_repo(explicit: Option<String>) -> anyhow::Result<String> {
    if let Some(r) = explicit {
        return Ok(r);
    }
    let cfg = backup::BackupConfig::load()?;
    cfg.github_repo.ok_or_else(|| {
        anyhow::anyhow!(
            "no repo URL given and none saved; pass --repo https://github.com/you/backup-repo"
        )
    })
}
