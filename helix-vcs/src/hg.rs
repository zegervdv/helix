use anyhow::{anyhow, Context, Result};
use arc_swap::ArcSwap;
use hg::revset;
use hg::utils::hg_path::HgPath;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::Arc;

use hg::config::Config;
use hg::operations::cat;
use hg::repo::Repo;

use crate::DiffProvider;

// #[cfg(test)]
// mod test;

pub struct Hg;

impl Hg {
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
            None => Hg::find_repo_root(path).context("cannot find root path")?,
        };

        match Repo::find(&non_repo_config, Some(repo_path).to_owned()) {
            Ok(repo) => Ok(repo),
            Err(_) => Err(anyhow!("failed to open hg repo")),
        }
    }
}

impl DiffProvider for Hg {
    fn get_diff_base(&self, file: &Path) -> Result<Vec<u8>> {
        debug_assert!(!file.exists() || file.is_file());
        debug_assert!(file.is_absolute());

        let repo_dir = file.parent().context("file has no parent directory")?;
        let repo = Hg::open_repo(repo_dir, None).context("failed to open hg repo")?;
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

    fn get_current_head_name(&self, file: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
        let repo_dir = file.parent().context("file has no parent directory")?;
        let repo = Hg::open_repo(repo_dir, None).context("failed to open hg repo")?;

        let rev = revset::resolve_single(".", &repo).map_err(|e| anyhow!("{:?}", e))?;
        let changelog = repo.changelog().map_err(|e| anyhow!("{:?}", e))?;
        let node = changelog.node_from_rev(rev.into());

        match node {
            Some(n) => {
                let rev = format!("{:x}", n);
                Ok(Arc::new(ArcSwap::from_pointee(
                    rev.to_owned().into_boxed_str(),
                )))
            }
            None => Err(anyhow!("could not find node")),
        }
    }
}
