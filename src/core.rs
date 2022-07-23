use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

pub type Env = BTreeMap<String, String>;

pub struct EnvrcContext {
    pub envrc: std::fs::File,
    pub root: PathBuf,
    pub env_cache_path: PathBuf,
    pub env_cache_dir: PathBuf,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to find .envrc in current or any parent directory")]
    NoEnvrc,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("failed to find QUICKENV_HOME or HOME")]
    NoQuickenvHome,
    #[error("failed to get current directory")]
    CurrentDir(#[source] io::Error),
}

pub fn resolve_envrc_context(quickenv_home: &Path) -> Result<EnvrcContext, Error> {
    let mut root = std::env::current_dir().map_err(Error::CurrentDir)?;

    let (envrc_path, envrc) = loop {
        let path = root.join(".envrc");
        if let Ok(f) = std::fs::File::open(&path) {
            log::debug!("loading {}", path.display());
            break (path, f);
        }

        if !root.pop() {
            return Err(Error::NoEnvrc);
        }
    };

    let env_cache_dir = quickenv_home.join("envs/");

    let mut env_hasher = blake3::Hasher::new();
    env_hasher.update(envrc_path.as_os_str().as_bytes());
    let env_cache_path = env_cache_dir.join(hex::encode(env_hasher.finalize().as_bytes()));

    Ok(EnvrcContext {
        root,
        env_cache_dir,
        envrc,
        env_cache_path,
    })
}

pub fn get_quickenv_home() -> Result<PathBuf, Error> {
    if let Ok(home) = std::env::var("QUICKENV_HOME") {
        Ok(Path::new(&home).to_owned())
    } else if let Ok(home) = std::env::var("HOME") {
        Ok(Path::new(&home).join(".quickenv/"))
    } else {
        Err(Error::NoQuickenvHome)
    }
}

pub fn get_envvars(quickenv_home: &Path) -> Result<Option<Env>, Error> {
    let ctx = resolve_envrc_context(quickenv_home)?;
    if let Ok(file) = std::fs::File::open(&ctx.env_cache_path) {
        let mut loaded_env_cache = BTreeMap::new();
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim_end_matches('\n');
            let (var_name, value) = line.split_once('=').unwrap_or((line, ""));
            loaded_env_cache.insert(var_name.to_owned(), value.to_owned());
        }

        return Ok(Some(loaded_env_cache));
    }

    Ok(None)
}
