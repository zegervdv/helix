use anyhow::{anyhow, Context, Result};
use arc_swap::ArcSwap;
use hg::matchers::AlwaysMatcher;
use hg::revset;
use hg::utils::hg_path::{hg_path_to_path_buf, HgPath};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, str};

use hg::config::Config;
use hg::dirstate::status::StatusPath;
use hg::operations::cat;
use hg::repo::Repo;
use hg::DirstateStatus;
use hg::PatternFileWarning;
use hg::StatusError;
use hg::StatusOptions;

use std::mem::take;

use crate::{DiffProvider, FileChange};

// #[cfg(test)]
// mod test;

fn find_repo_root(path: &Path) -> Result<PathBuf> {
    for ancestor in path.ancestors() {
        if ancestor.join(".hg").is_dir() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(anyhow!("cannot find root path"))
}

fn open_repo(path: &Path, root: Option<&Path>) -> Result<Repo> {
    let non_repo_config = Config::load_non_repo().unwrap();
    let repo_path = match root {
        Some(root_path) => root_path.to_path_buf(),
        None => find_repo_root(path).context("cannot find root path")?,
    };

    match Repo::find(&non_repo_config, Some(repo_path).to_owned()) {
        Ok(repo) => Ok(repo),
        Err(_) => Err(anyhow!("failed to open hg repo")),
    }
}

fn status(repo: &Repo, f: impl Fn(Result<FileChange>) -> bool) -> Result<()> {
    let mut dmap = repo.dirstate_map_mut().map_err(|e| anyhow!("{:?}", e))?;
    type StatusResult<'a> = Result<(DirstateStatus<'a>, Vec<PatternFileWarning>), StatusError>;

    let work_dir = repo.working_directory_path();

    let after_status = |res: StatusResult| -> Result<_> {
        let (ds_status, _) = res.map_err(|e| anyhow!("{:?}", e))?;

        let mut paths = Vec::new();

        paths.extend(ds_status.modified);
        paths.extend(ds_status.added);

        for StatusPath { path, copy_source } in paths {
            let path = work_dir.join(hg_path_to_path_buf(path).map_err(|e| anyhow!("{:?}", e))?);
            let change = match copy_source {
                Some(from) => FileChange::Renamed {
                    from_path: hg_path_to_path_buf(from).map_err(|e| anyhow!("{:?}", e))?,
                    to_path: path,
                },
                None => FileChange::Modified { path },
            };
            if !f(Ok(change)) {
                break;
            }
        }

        // Assume unsure means conflicted (might not always be true)
        for StatusPath { path, copy_source } in ds_status.unsure {
            let path = work_dir.join(hg_path_to_path_buf(path).map_err(|e| anyhow!("{:?}", e))?);
            if !f(Ok(FileChange::Conflict { path })) {
                break;
            }
        }

        for StatusPath { path, copy_source } in ds_status.removed {
            let path = work_dir.join(hg_path_to_path_buf(path).map_err(|e| anyhow!("{:?}", e))?);
            if !f(Ok(FileChange::Deleted { path })) {
                break;
            }
        }
        Ok(())
    };
    let options = StatusOptions {
        check_exec: true,
        list_clean: false,
        list_unknown: false,
        list_ignored: false,
        list_copies: true,
        collect_traversed_dirs: false,
    };

    dmap.with_status(
        &AlwaysMatcher,
        repo.working_directory_path().to_owned(),
        Vec::new(),
        options,
        after_status,
    )
    .map_err(|e| anyhow!("{:?}", e))?;
    Ok(())
}

pub fn get_diff_base(file: &Path) -> Result<Vec<u8>> {
    debug_assert!(!file.exists() || file.is_file());
    debug_assert!(file.is_absolute());

    if file.is_symlink() {
        return Err(anyhow!("symlinked file"))
    }
    let repo_dir = file.parent().context("file has no parent directory")?;
    let repo = open_repo(repo_dir, None).context("failed to open hg repo")?;
    let working_dir = repo.working_directory_path();

    let rev = ".";
    let files = vec![HgPath::new(
        file.strip_prefix(working_dir)?.to_str().unwrap(),
    )];

    match cat(&repo, &rev, files) {
        Err(e) => Err(anyhow!("failed to get file contents: {:?}", e)),
        Ok(result) => match result.results.get(0) {
            Some((_file, contents)) => return Ok(contents.to_vec()),
            None => Err(anyhow!("no such index")),
        },
    }
}

pub fn get_current_head_name(file: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
    let repo_dir = file.parent().context("file has no parent directory")?;
    let repo = open_repo(repo_dir, None).context("failed to open hg repo")?;

    let root = repo.working_directory_path().join(".hg");
    let mut branch = fs::read_to_string(root.join("branch")).context("branch has not been created (yet)")?;
    let topic_path = root.join("topic");
    if topic_path.exists() {
        let topic = fs::read_to_string(topic_path).context("topic has not been created (yet)")?;
        branch = format!("{}//{}", branch, topic);
    }

    Ok(Arc::new(ArcSwap::from_pointee(
        branch.to_owned().into_boxed_str(),
    )))
}

pub fn for_each_changed_file(cwd: &Path, f: impl Fn(Result<FileChange>) -> bool) -> Result<()> {
    status(&open_repo(cwd, None)?, f)
}
