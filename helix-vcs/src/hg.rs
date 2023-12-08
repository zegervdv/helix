use anyhow::{bail, Context, Result};
use arc_swap::ArcSwap;
use std::path::Path;
use std::sync::Arc;

use std::process::Command;

use crate::DiffProvider;

// #[cfg(test)]
// mod test;

pub struct Hg;

impl Hg {}

impl DiffProvider for Hg {
    fn get_diff_base(&self, file: &Path) -> Result<Vec<u8>> {
        debug_assert!(!file.exists() || file.is_file());
        debug_assert!(file.is_absolute());

        let mut hg = Command::new("hg");
        let cmd = hg
            .env("HGRCPATH", "")
            .arg("cat")
            .arg("--rev")
            .arg(".")
            .arg(file);

        let content = cmd.output().context("failed to open hg repo")?.stdout;

        Ok(content)
    }

    fn get_current_head_name(&self, file: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
        let mut hg = Command::new("hg");
        let cmd = hg
            .env("HGRCPATH", "")
            .arg("log")
            .arg("--rev")
            .arg(".")
            .arg("--template")
            .arg("{branch}");

        let branch = cmd.output().context("could not get branch name")?.stdout;
        Ok(Arc::new(ArcSwap::from_pointee(
            String::from_utf8(branch)?.into_boxed_str(),
        )))
    }
}
