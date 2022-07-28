use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub type Env = BTreeMap<OsString, OsString>;

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

pub fn parse_env_line(line: &[u8], env: &mut Env, prev_var_name: &mut Option<OsString>) {
    let mut split_iter = line.splitn(2, |&x| x == b'=');

    match split_iter
        .next()
        .and_then(|first| Some((first, split_iter.next()?)))
    {
        Some((var_name, value)) => {
            let var_name = OsString::from_vec(var_name.to_owned());
            let value = OsString::from_vec(value.to_owned());
            *prev_var_name = Some(var_name.clone());
            env.insert(var_name, value);
        }
        None => {
            let prev_value = env.get_mut(prev_var_name.as_ref().unwrap()).unwrap();
            prev_value.push(OsStr::new("\n"));
            prev_value.push(OsStr::from_bytes(line));
        }
    }
}

pub fn get_envvars(ctx: &EnvrcContext) -> Result<Option<Env>, Error> {
    if let Ok(file) = std::fs::File::open(&ctx.env_cache_path) {
        let mut loaded_env_cache = BTreeMap::new();
        let reader = BufReader::new(file);

        let mut prev_var_name = None;

        for line in reader.split(b'\n') {
            let raw_line = line?;
            let mut line = raw_line.as_slice();
            while let Some(b'\n') = line.last() {
                line = &line[..line.len()];
            }

            parse_env_line(line, &mut loaded_env_cache, &mut prev_var_name);
        }

        return Ok(Some(loaded_env_cache));
    }

    Ok(None)
}
