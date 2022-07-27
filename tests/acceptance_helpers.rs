use std::collections::BTreeMap;
use std::env::{current_dir, var};
use std::ffi::{OsStr, OsString};
use std::fs::{create_dir_all, set_permissions, Permissions};
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Error;
use tempfile::TempDir;

#[derive(Clone)]
pub struct Harness {
    pub env: BTreeMap<OsString, OsString>,
    pub home: Arc<TempDir>,
    pub cwd: PathBuf,
}

impl Harness {
    pub fn insta_settings(&self) -> insta::Settings {
        let mut insta_settings = insta::Settings::clone_current();
        // XXX(insta) Settings have to be static
        let slf = self.clone();
        insta_settings.add_dynamic_redaction(".**", move |value, _path| {
            if let Some(s) = value.as_str() {
                slf.scrub_output(s).unwrap().into()
            } else {
                value
            }
        });
        insta_settings
    }

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
        let home = self.var("HOME").unwrap().to_str().unwrap();
        let true_bin = which::which("true")?;
        let bash_bin = which::which("bash")?;
        let usr_bin = true_bin.parent().unwrap().to_str().unwrap();
        let usr_bin2 = bash_bin.parent().unwrap().to_str().unwrap();

        Ok(input
            .replace(&home, "[scrubbed $HOME]")
            .replace(usr_bin, "[scrubbed usr-bin]")
            .replace(usr_bin2, "[scrubbed usr-bin2]"))
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.cwd.join(path)
    }

    pub fn which(&self, binary_name: impl AsRef<OsStr>) -> which::Result<PathBuf> {
        which::which_in(binary_name, self.var("PATH"), &self.cwd)
    }

    pub fn cmd(&self, program_name: &str, args: Vec<&str>) -> Result<String, Error> {
        let child = Command::new(self.which(program_name)?)
            .current_dir(&self.cwd)
            .envs(&self.env)
            .args(args)
            .output()?;

        let stdout = self.scrub_output(&String::from_utf8(child.stdout)?)?;
        let stderr = self.scrub_output(&String::from_utf8(child.stderr)?)?;

        // more compact debug repr for insta
        Ok(format!(
            "status: {}\nstdout: {}\nstderr: {}",
            child.status.code().unwrap(),
            stdout,
            stderr
        ))
    }
}

pub fn setup() -> Result<Harness, Error> {
    let home = tempfile::tempdir()?;
    // on macos, /tmp is a symlink to /private/..., so sometimes the path reported by tmpdir is not
    // canonical
    let home_path = std::fs::canonicalize(home.path()).unwrap();
    let cwd = home_path.join("project/");
    create_dir_all(&cwd)?;
    let mut harness = Harness {
        env: BTreeMap::new(),
        home: Arc::new(home),
        cwd,
    };
    dbg!(&harness.home);

    create_dir_all(home_path.join(".quickenv/bin"))?;
    symlink(
        current_dir()?.join("target/debug/quickenv"),
        home_path.join(".quickenv/bin/quickenv"),
    )?;

    harness.set_var("HOME", &home_path);
    harness.set_var("PATH", var("PATH").unwrap());
    harness.prepend_path(home_path.join(".quickenv/bin"));
    Ok(harness)
}

pub fn set_executable(path: impl AsRef<Path>) -> Result<(), Error> {
    set_permissions(path, Permissions::from_mode(0o755))?;
    Ok(())
}

#[allow(unused_macros)]
macro_rules! assert_cmd {
    ($harness:expr, $argv0:ident $($arg:literal)*, $($insta_args:tt)*) => {{
        $harness.insta_settings().bind(|| {
            // XXX(insta): cannot use Result here
            insta_cmd::assert_cmd_snapshot!(
                Command::new($harness.which(stringify!($argv0)).unwrap())
                .current_dir(&$harness.cwd)
                .envs(&$harness.env)
                $(.arg($arg))*,
                $($insta_args)*
            );
        });
    }}
}

#[allow(unused_imports)]
pub(crate) use assert_cmd;
