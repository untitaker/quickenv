use std::collections::BTreeMap;
use std::env::{current_dir, var};
use std::ffi::{OsStr, OsString};
use std::fs::{create_dir_all, set_permissions, Permissions};
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Error;
use tempfile::TempDir;

pub struct Harness {
    pub env: BTreeMap<OsString, OsString>,
    pub home: TempDir,
    pub cwd: PathBuf,
}

impl Harness {
    pub fn prepend_path(&mut self, path: impl AsRef<OsStr>) {
        let mut new_path = path.as_ref().to_owned();
        new_path.push(":");
        new_path.push(self.var("PATH").unwrap());

        self.set_var("PATH", new_path);
    }

    pub fn var(&self, key: impl AsRef<OsStr>) -> Option<&OsStr> {
        self.env.get(key.as_ref()).map(OsString::as_os_str)
    }

    pub fn set_var(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) {
        self.env
            .insert(key.as_ref().to_owned(), value.as_ref().to_owned());
    }

    pub fn scrub_output(&self, input: &str) -> Result<String, Error> {
        let home = self.var("HOME").unwrap();
        let true_bin = which::which("true")?;
        let bash_bin = which::which("bash")?;
        let usr_bin = true_bin.parent().unwrap().to_str().unwrap();
        let usr_bin2 = bash_bin.parent().unwrap().to_str().unwrap();

        Ok(input
            .replace(&home.to_str().unwrap(), "[scrubbed $HOME]")
            .replace(usr_bin, "[scrubbed usr-bin]")
            .replace(usr_bin2, "[scrubbed usr-bin2]"))
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.cwd.join(path)
    }

    pub fn which(&self, binary_name: impl AsRef<OsStr>) -> which::Result<PathBuf> {
        which::which_in(binary_name, self.var("PATH"), &self.cwd)
    }
}

pub fn setup() -> Result<Harness, Error> {
    let home = tempfile::tempdir()?;
    let cwd = home.path().join("project/").to_owned();
    create_dir_all(&cwd)?;
    let mut harness = Harness {
        env: BTreeMap::new(),
        home,
        cwd,
    };
    dbg!(&harness.home);

    create_dir_all(harness.home.path().join(".quickenv/bin"))?;
    symlink(
        current_dir()?.join("target/debug/quickenv"),
        harness.home.path().join(".quickenv/bin/quickenv"),
    )?;

    harness.set_var("HOME", harness.home.path().to_owned());
    harness.set_var("PATH", var("PATH").unwrap());
    harness.prepend_path(harness.home.path().join(".quickenv/bin"));
    Ok(harness)
}

pub fn set_executable(path: impl AsRef<Path>) -> Result<(), Error> {
    set_permissions(path, Permissions::from_mode(0o755))?;
    Ok(())
}

#[allow(unused_macros)]
macro_rules! assert_cmd {
    ($harness:expr, $argv0:ident $($arg:literal)*, $($insta_args:tt)*) => {{
        let command = Command::new($harness.which(stringify!($argv0))?)
            .current_dir(&$harness.cwd)
            .envs(&$harness.env)
            $(.arg($arg))*;

        insta_cmd::assert_cmd_snapshot!(command, {
            ".**" => insta::dynamic_redaction(|value, _path| {
                $harness.scrub_output(value).unwrap()
            }),
        }, $($insta_args:tt)*);
    }}
}

#[allow(unused_imports)]
pub(crate) use assert_cmd;
