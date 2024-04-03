use anyhow::{bail, Context, Result};
use arc_swap::ArcSwap;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::Arc;

use std::process::Command;

use crate::{DiffProvider, FileChange};

#[cfg(test)]
mod test;

#[derive(Clone, Copy)]
pub struct Hg;

fn exec_hg_cmd_raw(bin: &str, args: &str, root: Option<&str>) -> Result<Vec<u8>> {
    let mut cmd = Command::new(bin);

    cmd.env("HGPLAIN", "").env("HGRCPATH", "");

    if let Some(dir) = root {
        cmd.arg("--cwd").arg(dir);
    }

    cmd.args(args.split_whitespace());

    match cmd.output() {
        Ok(result) => Ok(result.stdout),
        Err(e) => bail!("`hg {args}` failed: {}", e),
    }
}

fn exec_hg_cmd(bin: &str, args: &str, root: Option<&str>) -> Result<String> {
    match exec_hg_cmd_raw(bin, args, root) {
        Ok(result) => {
            Ok(String::from_utf8(result).context("Failed to parse output of `hg {args}`")?)
        }
        Err(e) => Err(e),
    }
}

impl Hg {
    fn get_repo_root(path: &Path) -> Result<PathBuf> {
        if path.is_symlink() {
            bail!("ignoring symlinks");
        };

        let workdir = if path.is_dir() {
            path
        } else {
            path.parent().context("path has no parent")?
        };

        match exec_hg_cmd("rhg", "root", workdir.to_str()) {
            Ok(output) => {
                let root = output
                    .strip_suffix("\n")
                    .or(output.strip_suffix("\r\n"))
                    .unwrap_or(output.as_str());

                if root.is_empty() {
                    bail!("did not find root")
                };

                if path.is_dir() {
                    Ok(Path::new(&root).to_path_buf())
                } else {
                    let arg = format!("files {}", path.to_str().unwrap());
                    match exec_hg_cmd("rhg", &arg, Some(root)) {
                        Ok(output) => {
                            let tracked = output
                                .strip_suffix("\n")
                                .or(output.strip_suffix("\r\n"))
                                .unwrap_or(output.as_str());

                            if (output.len() > 0)
                                && (Path::new(tracked) == path.strip_prefix(root).unwrap())
                            {
                                Ok(Path::new(&root).to_path_buf())
                            } else {
                                bail!("not a tracked file")
                            }
                        }
                        Err(_) => bail!("not a tracked file"),
                    }
                }
            }
            Err(_) => bail!("not in a hg repo"),
        }
    }

    fn status(cwd: &Path, f: impl Fn(Result<FileChange>) -> bool) -> Result<()> {
        let root = Self::get_repo_root(cwd).context("not a hg repo")?;
        let arg = format!("status {} --copies -Tjson", cwd.to_str().unwrap());
        let content =
            exec_hg_cmd_raw("hg", &arg, root.to_str()).context("could not get file content")?;
        let json: serde_json::Value =
            serde_json::from_str(str::from_utf8(&content)?).context("invalid status response")?;

        if let Some(states) = json.as_array() {
            for state in states {
                let change = match state["status"].as_str().unwrap() {
                    "M" => {
                        let path = PathBuf::from(state["path"].as_str().unwrap());
                        if !state["unresolved"].as_bool().is_none() {
                            FileChange::Conflict { path }
                        } else {
                            FileChange::Modified { path }
                        }
                    }
                    "R" => {
                        let path = PathBuf::from(state["path"].as_str().unwrap());
                        FileChange::Deleted { path }
                    }
                    "A" => {
                        let path = PathBuf::from(state["path"].as_str().unwrap());
                        match state["source"].as_str() {
                            Some(source) => {
                                let source_path = PathBuf::from(source);
                                FileChange::Renamed {
                                    from_path: source_path,
                                    to_path: path,
                                }
                            }
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
                if !f(Ok(change)) {
                    break;
                }
            }
        }

        Ok(())
    }
}

impl Hg {
    pub fn get_diff_base(&self, file: &Path) -> Result<Vec<u8>> {
        debug_assert!(!file.exists() || file.is_file());
        debug_assert!(file.is_absolute());

        let root = Hg::get_repo_root(file).context("not a hg repo")?;

        let arg = format!("cat --rev=. {}", file.to_str().unwrap());
        let content =
            exec_hg_cmd_raw("rhg", &arg, root.to_str()).context("could not get file content")?;

        Ok(content)
    }

    pub fn get_current_head_name(&self, file: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
        debug_assert!(!file.exists() || file.is_file());
        debug_assert!(file.is_absolute());

        let root = Hg::get_repo_root(file).context("not a hg repo")?;

        let branch = exec_hg_cmd(
            "hg",
            "--config extensions.evolve= log --rev=wdir() --template={branch}",
            root.to_str(),
        )
        .context("could not get branch name")?;
        Ok(Arc::new(ArcSwap::from_pointee(branch.into_boxed_str())))
    }

    pub fn for_each_changed_file(
        &self,
        cwd: &Path,
        f: impl Fn(Result<FileChange>) -> bool,
    ) -> Result<()> {
        Self::status(cwd, f)
    }
}

impl From<Hg> for DiffProvider {
    fn from(value: Hg) -> Self {
        DiffProvider::Hg(value)
    }
}
