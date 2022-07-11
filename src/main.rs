use std::collections::{BTreeMap, BTreeSet};
use std::env::VarError;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::{self, Stdio};

use anyhow::Error as BoxError;
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Parser, Debug)]
enum Command {
    /// Execute .envrc in the current or parent directory, and store the resulting environment
    /// variables.
    Reload,
    /// Dump out cached environment variables.
    ///
    /// For example, use 'quickenv reload && eval "$(quickenv vars)"' to load the environment like
    /// direnv normally would.
    Vars,
    /// Create a new shim binary in ~/.quickenv/bin/.
    ///
    /// Executing that binary will run in the context of the nearest .envrc, as if it was activated
    /// by direnv.
    Shim {
        #[clap(value_parser)]
        /// The names of the commands to expose.
        commands: Vec<String>,
    },
    /// Remove a shim binary from ~/.quickenv/bin/.
    Unshim {
        #[clap(value_parser)]
        /// The names of the commands to remove.
        commands: Vec<String>,
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("failed to find .envrc in current or any parent directory")]
    NoEnvrc,

    #[error("{0}")]
    Io(#[from] io::Error),

    #[error("{0}")]
    Exec(#[from] exec::Error),

    #[error("{0}")]
    Which(#[from] which::Error),

    #[error("{0}")]
    Var(#[from] VarError),

    #[error("{0}")]
    Other(#[from] BoxError),
}

fn main() {
    match main_inner() {
        Ok(()) => (),
        Err(e) => {
            log::error!("{}", e);
            std::process::exit(1);
        }
    }
}

fn main_inner() -> Result<(), Error> {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    check_for_shim()?;

    let args = Args::parse();

    match args.subcommand {
        Command::Reload => command_reload(),
        Command::Vars => command_vars(),
        Command::Shim { commands } => command_shim(commands),
        Command::Unshim { commands } => command_unshim(commands),
    }
}

fn safe_command(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
    command: &str,
    mut f: impl FnMut(&str) -> Result<(), Error>,
) -> Result<(), Error> {
    stdin.write(b"\necho //QUICKENV-BEGIN\n")?;
    stdin.write(command.as_bytes())?;
    stdin.write(b"\necho //QUICKENV-END\n")?;
    let mut found_begin = false;

    for line in stdout.lines() {
        let line = line?;
        let line = line.trim_end_matches('\n');

        if !found_begin {
            if line == "//QUICKENV-BEGIN" {
                found_begin = true;
            } else {
                println!("{line}");
            }

            continue;
        }

        if line == "//QUICKENV-END" {
            break;
        }

        f(line)?;
    }

    Ok(())
}

fn dump_env(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
) -> Result<BTreeMap<String, String>, Error> {
    let mut environ = BTreeMap::new();

    safe_command(stdin, stdout, "env", |line| {
        let (var_name, value) = line.split_once('=').unwrap_or((line, ""));
        environ.insert(var_name.to_owned(), value.to_owned());
        Ok(())
    })?;

    Ok(environ)
}

fn get_quickenv_home() -> Result<PathBuf, Error> {
    let rv = home::home_dir()
        .ok_or(anyhow::anyhow!("failed to find your HOME dir"))?
        .join(".quickenv/");
    Ok(rv)
}

fn compute_envvars() -> Result<(), Error> {
    let mut ctx = resolve_envrc_context()?;

    let mut cmd = process::Command::new("bash")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .current_dir(ctx.root)
        .spawn()?;

    let mut stdin = cmd.stdin.take().unwrap();
    let mut stdout_buf = BufReader::new(cmd.stdout.take().unwrap());
    let old_env = dump_env(&mut stdin, &mut stdout_buf)?;
    stdin.write(b"eval \"$(direnv stdlib)\"\n")?;

    io::copy(&mut ctx.envrc, &mut stdin)?;

    let new_env = dump_env(&mut stdin, &mut stdout_buf)?;

    std::fs::create_dir_all(ctx.env_cache_dir)?;
    let mut env_cache = BufWriter::new(std::fs::File::create(ctx.env_cache_path)?);

    for (key, value) in new_env {
        if old_env.get(&key) != Some(&value) {
            write!(&mut env_cache, "{key}={value}\n")?;
        }
    }

    Ok(())
}

struct EnvrcContext {
    envrc: std::fs::File,
    root: PathBuf,
    env_cache_path: PathBuf,
    env_cache_dir: PathBuf,
}

fn resolve_envrc_context() -> Result<EnvrcContext, Error> {
    let mut root = std::env::current_dir()?;

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

    let env_cache_dir = get_quickenv_home()?.join("envs/");

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

fn get_envvars() -> Result<Option<BTreeMap<String, String>>, Error> {
    let ctx = resolve_envrc_context()?;
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


fn get_new_paths(path_envvar: &str) -> Result<BTreeSet<PathBuf>, Error> {
    let own_path = std::env::var("PATH")?;
    let current_paths = std::env::split_paths(&own_path)
        .filter_map(|x| std::fs::canonicalize(x).ok())
        .collect::<BTreeSet<PathBuf>>();

    let new_paths = std::env::split_paths(&path_envvar)
        .filter_map(|x| std::fs::canonicalize(x).ok())
        .filter(|path| !current_paths.contains(path))
        .collect::<BTreeSet<PathBuf>>();

    Ok(new_paths)
}

fn command_reload() -> Result<(), Error> {
    compute_envvars()?;
    let envvars = get_envvars()?.expect("somehow didn't end up writing envvars");
    if let Some(path_envvar) = envvars.get("PATH") {
        let paths = get_new_paths(&path_envvar)?;
        if !paths.is_empty() {
            log::info!("{} new entries in PATH. use 'quickenv shim <command>' to put a shim binary into your global PATH", paths.len());
            for path in paths {
                println!("{}", path.display());
            }
        }
    }

    Ok(())
}

fn command_vars() -> Result<(), Error> {
    if let Some(envvars) = get_envvars()? {
        for (k, v) in envvars {
            println!("{k}={v}");
        }

        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "run 'quickenv reload' first to generate envvars"
        ))?
    }
}

fn command_shim(commands: Vec<String>) -> Result<(), Error> {
    let quickenv_dir = get_quickenv_home()?;
    let bin_dir = quickenv_dir.join("bin/");
    std::fs::create_dir_all(&bin_dir)?;

    let self_binary = bin_dir.join("quickenv");

    let mut changes = 0;

    for command in &commands {
        let command_path = bin_dir.join(command);
        let was_there = std::fs::remove_file(&command_path).is_ok();
        symlink(&self_binary, &command_path)?;

        if !was_there {
            changes += 1;
        }

        let effective_command_path = which::which(command)?;

        if effective_command_path != command_path {
            Err(anyhow::anyhow!(
                "{} is shadowed by an executable of the same name at {}",
                command_path.display(),
                effective_command_path.display(),
            ))?
        }
    }

    log::info!("created {} new shims in ~/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again", changes);

    Ok(())
}

fn command_unshim(commands: Vec<String>) -> Result<(), Error> {
    let quickenv_dir = get_quickenv_home()?;
    let bin_dir = quickenv_dir.join("bin/");
    let mut changes = 0;
    for command in &commands {
        let command_path = bin_dir.join(command);
        if std::fs::remove_file(&command_path).is_ok() {
            changes += 1;
        }
    }

    log::info!("removed {} shims from ~/.quickenv/bin/. Use 'quickenv shim <command>' to add them again", changes);

    Ok(())
}

fn check_for_shim() -> Result<(), Error> {
    let program_name = std::env::args()
        .next()
        .ok_or(anyhow::anyhow!("failed to determine own program name"))?;

    if program_name == "quickenv" {
        return Ok(());
    }

    let own_path = which::which(&program_name)?;

    match get_envvars() {
        Ok(None) => {
            Err(anyhow::anyhow!("run 'quickenv reload' first to generate envvars"))?
        }
        Ok(Some(envvars)) => {
            for (k, v) in envvars {
                std::env::set_var(k, v);
            }
        },
        Err(Error::NoEnvrc) => (),
        Err(e) => {
            return Err(e);
        }
    }

    for path in which::which_all(&program_name)? {
        if path == own_path {
            continue;
        }

        log::debug!("execvp {}", path.display());

        Err(exec::execvp(path, std::env::args()))?
    }

    Err(anyhow::anyhow!("failed to find {program_name} on path").into())
}
