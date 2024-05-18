use std::collections::{BTreeMap, BTreeSet};

use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, BufWriter, Write};

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};

use log::{Level, LevelFilter};

use anyhow::{Context, Error};
use clap::Parser;
use console::style;

mod core;
mod grid;
mod signals;

use crate::core::resolve_envrc_context;

// Disabling colored help because the after_help isn't colored, for consistency
#[derive(Parser, Debug)]
#[clap(
    version,
    about,
    disable_colored_help = true,
    after_help = "ENVIRONMENT VARIABLES:
    QUICKENV_LOG=debug to enable debug output (in shim commands as well)
    QUICKENV_LOG=error to silence everything but errors
    QUICKENV_NO_SHIM=1 to disable loading of .envrc, and effectively disable shims
    QUICKENV_SHIM_EXEC=1 to directly exec() shims instead of spawning them as subprocess. This can help with attaching debuggers.
    QUICKENV_NO_SHIM_WARNINGS=1 to disable nags about running 'quickenv shim' everytime a new binary is added
    QUICKENV_PRELUDE='eval \"$(direnv stdlib)\"' can be overridden to something else to get rid of the direnv stdlib and therefore direnv dependency, or to inject additional code before executing each envrc.
"
)]
struct Args {
    #[clap(subcommand)]
    subcommand: Command,
}

#[derive(Parser, Debug)]
enum Command {
    /// Execute .envrc in the current or parent directory, and cache the new variables.
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
    ///
    /// If no commands are provided, quickenv will determine which commands the current .envrc
    /// makes available, ask for confirmation, and create shims for those commands.
    ///
    /// If commands are provided, quickenv creates those shims directly without confirmation.
    Shim {
        /// Disable confirmation prompts when running 'shim' without arguments.
        #[clap(long, short)]
        yes: bool,
        /// The names of the commands to expose. If missing, quickenv will determine recommended
        /// commands itself and ask for confirmation.
        commands: Vec<String>,
    },
    /// Remove a shim binary from ~/.quickenv/bin/.
    Unshim {
        /// The names of the commands to remove.
        commands: Vec<String>,
    },
    /// Run a program with .envrc loaded without having to shim it.
    Exec {
        program_name: OsString,
        #[clap(allow_hyphen_values = true, trailing_var_arg = true)]
        args: Vec<OsString>,
    },
    /// Determine which program quickenv's shim would launch under the hood.
    ///
    /// This will error if the shim is not installed. Pass '--pretend-shimmed' to simulate what would
    /// happen anyway.
    Which {
        /// The command name to look up.
        program_name: OsString,

        /// If quickenv does not have a shim under the given program name, this command errors by
        /// default. This check can be disabled using '--pretend-shimmed'
        #[clap(long)]
        pretend_shimmed: bool,
    },
}

fn main() {
    match main_inner() {
        Ok(()) => (),
        Err(e) => {
            log::error!("{:?}", e);
            std::process::exit(1);
        }
    }
}

fn main_inner() -> Result<(), Error> {
    env_logger::Builder::new()
        .format(|buf, record| match record.level() {
            Level::Info => writeln!(buf, "{}", record.args()),
            // We're adding "quickenv" to every line here on purpose, because it makes debugging
            // shims much less confusing, where it's not always clear which piece of software
            // emitted which line.
            Level::Warn => writeln!(
                buf,
                "[{} quickenv] {}",
                style("WARN").yellow(),
                record.args()
            ),
            Level::Error => writeln!(buf, "[{} quickenv] {}", style("ERROR").red(), record.args()),
            Level::Debug => writeln!(
                buf,
                "[{} quickenv] {}",
                style("DEBUG").blue(),
                record.args()
            ),
            Level::Trace => writeln!(
                buf,
                "[{} quickenv] {}",
                style("TRACE").magenta(),
                record.args()
            ),
        })
        .filter_level(LevelFilter::Info)
        .parse_env("QUICKENV_LOG")
        .init();

    check_for_shim().context("failed to run shimmed command")?;

    let args = Args::parse();

    crate::signals::set_ctrlc_handler()?;

    match args.subcommand {
        Command::Reload => command_reload(),
        Command::Vars => command_vars(),
        Command::Shim { commands, yes } => command_shim(commands, yes),
        Command::Unshim { commands } => command_unshim(commands),
        Command::Exec { program_name, args } => command_exec(program_name, args),
        Command::Which {
            program_name,
            pretend_shimmed,
        } => command_which(program_name, pretend_shimmed),
    }
}

#[derive(Clone, Copy)]
enum ParseState {
    PreBefore,
    InBefore,
    PreAfter,
    InAfter,
    End,
}

fn parse_env_diff<R: BufRead>(
    reader: R,
    mut script_output: impl FnMut(&[u8]) -> Result<(), Error>,
) -> Result<(core::Env, core::Env), Error> {
    let mut parse_state = ParseState::PreBefore;
    let mut old_env = BTreeMap::new();
    let mut new_env = BTreeMap::new();
    let mut prev_var_name = None;

    for line in reader.split(b'\n') {
        let raw_line = line?;
        let mut line = raw_line.as_slice();
        while let Some(b'\n') = line.last() {
            line = &line[..line.len()];
        }

        match (parse_state, line) {
            (ParseState::PreBefore, b"// BEGIN QUICKENV-BEFORE") => {
                prev_var_name = None;
                parse_state = ParseState::InBefore;
            }
            (ParseState::InBefore, b"// END QUICKENV-BEFORE") => {
                prev_var_name = None;
                parse_state = ParseState::PreAfter;
            }
            (ParseState::PreAfter, b"// BEGIN QUICKENV-AFTER") => {
                prev_var_name = None;
                parse_state = ParseState::InAfter;
            }
            (ParseState::InAfter, b"// END QUICKENV-AFTER") => {
                prev_var_name = None;
                parse_state = ParseState::End;
            }
            (ParseState::InBefore, line) => {
                core::parse_env_line(line, &mut old_env, &mut prev_var_name);
            }
            (ParseState::InAfter, line) => {
                core::parse_env_line(line, &mut new_env, &mut prev_var_name);
            }
            (_, _) => {
                script_output(&raw_line)?;
            }
        }
    }

    Ok((old_env, new_env))
}

#[test]
fn test_parse_env_diff() {
    let input = br#"
some output 1
// BEGIN QUICKENV-BEFORE
hello=world
bogus=wogus
// END QUICKENV-BEFORE
some output 2
// BEGIN QUICKENV-AFTER
hello=world
bogus=wogus
2
more=keys
// END QUICKENV-AFTER
some output 3
"#;

    let mut output: Vec<Vec<u8>> = Vec::new();
    let (old_env, new_env) = parse_env_diff(input.as_slice(), |line| {
        output.push(line.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        old_env,
        maplit::btreemap![
            "hello".into() => "world".into(),
            "bogus".into() => "wogus".into(),
        ]
    );

    assert_eq!(
        new_env,
        maplit::btreemap![
            "hello".into() => "world".into(),
            "bogus".into() => "wogus\n2".into(),
            "more".into() => "keys".into(),
        ]
    );

    assert_eq!(
        output,
        vec![
            b"".as_slice().to_owned(),
            b"some output 1".as_slice().to_owned(),
            b"some output 2".as_slice().to_owned(),
            b"some output 3".as_slice().to_owned()
        ]
    );
}

fn compute_envvars(quickenv_home: &Path) -> Result<(), Error> {
    let mut ctx = crate::core::resolve_envrc_context(quickenv_home)?;
    std::fs::create_dir_all(&ctx.env_cache_dir).with_context(|| {
        format!(
            "failed to create cache directory at {}",
            &ctx.env_cache_dir.display()
        )
    })?;
    let mut temp_script = tempfile::NamedTempFile::new_in(&ctx.root)
        .with_context(|| format!("failed to create temporary file at {}", ctx.root.display()))?;
    let temp_script_path = temp_script.path().to_owned();

    let write_failure = || {
        format!(
            "failed to write to temporary file at {}",
            temp_script_path.display()
        )
    };

    let prelude = std::env::var("QUICKENV_PRELUDE")
        .unwrap_or_else(|_| r#"eval "$(direnv stdlib)""#.to_owned());

    write!(
        temp_script,
        r##"
echo '// BEGIN QUICKENV-BEFORE'
env
echo '// END QUICKENV-BEFORE'
{prelude}
"##,
    )
    .with_context(write_failure)?;

    io::copy(&mut ctx.envrc, &mut temp_script).with_context(write_failure)?;

    write!(
        temp_script,
        r##"
echo '// BEGIN QUICKENV-AFTER'
env
echo '// END QUICKENV-AFTER'
"##
    )
    .with_context(write_failure)?;

    signals::pass_control_to_shim();

    let mut cmd = process::Command::new("bash")
        .arg(&temp_script_path)
        .env("QUICKENV_NO_SHIM", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .current_dir(ctx.root)
        .spawn()
        .context("failed to spawn bash for running envrc")?;

    let stdout_buf = BufReader::new(cmd.stdout.take().unwrap());
    let (old_env, new_env) = parse_env_diff(stdout_buf, |line| {
        io::stdout().write_all(line)?;
        io::stdout().write_all(b"\n")?;
        Ok(())
    })
    .context("failed to parse envrc output")?;

    let status = cmd.wait().context("failed to wait for envrc subprocess")?;

    if !status.success() {
        Err(anyhow::anyhow!(".envrc exited with status {status}"))?;
    }

    let mut env_cache =
        BufWriter::new(std::fs::File::create(&ctx.env_cache_path).with_context(|| {
            format!(
                "failed to create envrc cache at {}",
                &ctx.env_cache_path.display()
            )
        })?);

    for (key, value) in new_env {
        if old_env.get(&key) != Some(&value) {
            env_cache.write_all(key.as_bytes())?;
            env_cache.write_all(b"=")?;
            env_cache.write_all(value.as_bytes())?;
            env_cache.write_all(b"\n")?;
        }
    }

    Ok(())
}

fn get_missing_shims(
    quickenv_home: &Path,
    new_path_envvar: Option<&OsStr>,
) -> Result<BTreeSet<String>, Error> {
    let mut rv = BTreeSet::new();
    let new_path_envvar = match new_path_envvar {
        Some(x) => x,
        None => return Ok(rv),
    };

    let old_paths = std::env::var("PATH").context("failed to read PATH")?;
    let old_paths = std::env::split_paths(&old_paths)
        .map(|x| std::fs::canonicalize(&x).unwrap_or(x))
        .collect::<BTreeSet<PathBuf>>();

    for directory in std::env::split_paths(new_path_envvar) {
        let directory = std::fs::canonicalize(&directory).unwrap_or(directory);
        if old_paths.contains(&directory) {
            continue;
        }

        match get_missing_shims_from_dir(quickenv_home, &directory, &mut rv) {
            Ok(()) => (),
            Err(e) => {
                log::debug!("skipping over directory {:?}: {:?}", directory, e);
                continue;
            }
        }
    }

    Ok(rv)
}

fn get_missing_shims_from_dir(
    quickenv_home: &Path,
    path: &Path,
    rv: &mut BTreeSet<String>,
) -> Result<(), Error> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            // directories have the executable bit set, so we should skip them explicitly.
            continue;
        }
        let permissions = metadata.permissions();
        let is_executable = permissions.mode() & 0o111 != 0;
        if !is_executable {
            continue;
        }

        let entry_path = entry.path();

        let filename = match entry_path.file_name().and_then(|x| x.to_str()) {
            Some(x) => x,
            None => continue,
        };

        if !quickenv_home.join("bin").join(filename).exists() {
            rv.insert(filename.to_owned());
        }
    }

    Ok(())
}

fn command_reload() -> Result<(), Error> {
    let quickenv_home = crate::core::get_quickenv_home()?;
    let mut unshimmed_commands = CheckUnshimmedCommands::new(&quickenv_home)?;
    unshimmed_commands.exclude_current()?;
    compute_envvars(&quickenv_home)?;
    unshimmed_commands.check_unshimmed_commands(false)?;

    Ok(())
}

enum CheckUnshimmedCommands<'a> {
    Enabled {
        ctx: core::EnvrcContext,
        quickenv_home: &'a Path,
        old_missing_shims: BTreeSet<String>,
    },
    Disabled,
}

impl<'a> CheckUnshimmedCommands<'a> {
    fn new(quickenv_home: &'a Path) -> Result<Self, Error> {
        if std::env::var("QUICKENV_NO_SHIM_WARNINGS").unwrap_or_default() == "1" {
            Ok(CheckUnshimmedCommands::Disabled)
        } else {
            Ok(CheckUnshimmedCommands::Enabled {
                ctx: resolve_envrc_context(quickenv_home)?,
                quickenv_home,
                old_missing_shims: BTreeSet::new(),
            })
        }
    }

    fn exclude_current(&mut self) -> Result<(), Error> {
        match self {
            CheckUnshimmedCommands::Enabled {
                ctx,
                quickenv_home,
                ref mut old_missing_shims,
            } => {
                let envvars = match crate::core::get_envvars(ctx)? {
                    Some(x) => x,
                    None => return Ok(()),
                };

                let new_path_envvar = envvars.get(OsStr::new("PATH")).map(OsString::as_os_str);

                *old_missing_shims = get_missing_shims(quickenv_home, new_path_envvar)?;
            }
            CheckUnshimmedCommands::Disabled => (),
        }

        Ok(())
    }

    fn check_unshimmed_commands(self, only_if_new: bool) -> Result<(), Error> {
        match self {
            CheckUnshimmedCommands::Enabled {
                ctx,
                quickenv_home,
                old_missing_shims,
            } => {
                let envvars = match crate::core::get_envvars(&ctx)? {
                    Some(x) => x,
                    None => return Ok(()),
                };

                let new_path_envvar = envvars.get(OsStr::new("PATH")).map(OsString::as_os_str);
                let mut missing_shims = get_missing_shims(quickenv_home, new_path_envvar)?;
                let total_missing_shims = missing_shims.len();

                for elem in &old_missing_shims {
                    missing_shims.remove(elem);
                }

                let new_missing_shims = missing_shims.len();

                if (total_missing_shims > 0 && !only_if_new)
                    || (new_missing_shims > 0 && only_if_new)
                {
                    let new_shims_txt = if new_missing_shims > 0 {
                        format!(" ({} new)", style(new_missing_shims).green())
                    } else {
                        String::new()
                    };

                    log::warn!(
                        "{} unshimmed commands{}. Use {} to make them available.\n\
                        Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.",
                        style(total_missing_shims).green(),
                        new_shims_txt,
                        style("'quickenv shim'").magenta(),
                    )
                }
            }
            CheckUnshimmedCommands::Disabled => (),
        }

        Ok(())
    }
}

fn command_vars() -> Result<(), Error> {
    let quickenv_home = crate::core::get_quickenv_home()?;
    let ctx = resolve_envrc_context(&quickenv_home)?;

    if let Some(envvars) = core::get_envvars(&ctx)? {
        for (k, v) in envvars {
            io::stdout().write_all(k.as_bytes())?;
            io::stdout().write_all(b"=")?;
            io::stdout().write_all(v.as_bytes())?;
            io::stdout().write_all(b"\n")?;
        }

        Ok(())
    } else {
        log::error!(
            "Run {} first to generate envvars",
            style("'quickenv reload'").magenta()
        );
        std::process::exit(1);
    }
}

fn command_shim(mut commands: Vec<String>, yes: bool) -> Result<(), Error> {
    let quickenv_home = crate::core::get_quickenv_home()?;
    let bin_dir = quickenv_home.join("bin/");

    let auto = commands.is_empty();

    if auto {
        let ctx = resolve_envrc_context(&quickenv_home)?;
        let envvars = match crate::core::get_envvars(&ctx)? {
            Some(x) => x,
            None => {
                log::error!(
                    "Run {} first to generate envvars",
                    style("'quickenv reload'").magenta()
                );
                std::process::exit(1);
            }
        };
        let path_envvar = envvars.get(OsStr::new("PATH")).map(OsString::as_os_str);
        commands = get_missing_shims(&quickenv_home, path_envvar)?
            .into_iter()
            .collect();

        if !commands.is_empty() {
            eprintln!(
                "Found these unshimmed commands in your {}:",
                style(".envrc").cyan()
            );
            eprintln!();
            grid::print_as_grid(&commands);
            eprintln!();
            if commands.len() == 1 {
                eprintln!(
                    "Quickenv will create this new shim binary in {}.",
                    style(bin_dir.display()).cyan()
                );
            } else {
                eprintln!(
                    "Quickenv will create these {} new shim binaries in {}.",
                    style(commands.len()).green(),
                    style(bin_dir.display()).cyan()
                );
            }
            eprintln!(
                "Inside of {}, those commands will run with {} enabled.",
                style(ctx.root.display()).cyan(),
                style(".envrc").cyan()
            );
            eprintln!("Outside, they will run normally.");

            if !yes {
                let answer = dialoguer::Confirm::new()
                    .with_prompt(style("Continue?").red().to_string())
                    .default(true)
                    .interact()?;

                if !answer {
                    std::process::exit(1);
                }

                eprintln!();
            }
        }
    }

    std::fs::create_dir_all(&bin_dir)?;

    let self_binary = which::which("quickenv")?;

    let mut changes = 0;

    for command in &commands {
        if command == "quickenv" {
            log::warn!("not shimming own binary");
            continue;
        }

        let command_path = bin_dir.join(command);

        let was_there = std::fs::remove_file(&command_path).is_ok();
        symlink(&self_binary, &command_path).with_context(|| {
            format!(
                "failed to symlink {} to {}",
                self_binary.display(),
                command_path.display()
            )
        })?;

        if !was_there {
            changes += 1;
        }

        let effective_command_path = which::which(command).with_context(|| {
            format!(
                "failed to find command {} after shimming. Are you sure that {} is on your PATH?",
                bin_dir.display(),
                command
            )
        })?;

        if effective_command_path != command_path {
            log::error!(
                "{} is shadowed by an executable of the same name at {}",
                style(command_path.display()).cyan(),
                style(effective_command_path.display()).magenta(),
            );
            std::process::exit(1);
        }
    }

    if changes == 0 {
        log::info!("created {} new shims.", style("no").red());
    } else {
        log::info!(
            "Created {} new shims in {}.",
            style(changes).green(),
            style(bin_dir.display()).cyan(),
        );
        log::info!(
            "Use {} to remove them again.",
            style("'quickenv unshim <command>'").magenta(),
        );
    }

    if auto {
        log::info!(
            "Use {} to run additional commands with {} enabled.",
            style("'quickenv shim <command>'").magenta(),
            style(".envrc").cyan()
        );
    }

    Ok(())
}

fn command_unshim(commands: Vec<String>) -> Result<(), Error> {
    let quickenv_dir = crate::core::get_quickenv_home()?;
    let bin_dir = quickenv_dir.join("bin/");
    let mut changes = 0;
    for command in &commands {
        if command == "quickenv" {
            log::warn!("not unshimming own binary");
            continue;
        }

        let command_path = bin_dir.join(command);
        if std::fs::remove_file(&command_path).is_ok() {
            changes += 1;
        }
    }

    log::info!(
        "Removed {} shims from {}.\nUse {} to add them again",
        style(changes).green(),
        style(bin_dir.display()).cyan(),
        style("'quickenv shim <command>'").magenta(),
    );

    Ok(())
}

fn exec_shimmed_binary(program_name: &OsStr, args: Vec<OsString>) -> Result<(), Error> {
    log::debug!("attempting to launch shim for {:?}", program_name);

    let quickenv_home = crate::core::get_quickenv_home()?;
    let shimmed_binary_result = find_shimmed_binary(&quickenv_home, program_name)
        .context("failed to find actual binary")?;

    if std::env::var("QUICKENV_SHIM_EXEC").unwrap_or_default() == "1" {
        for (k, v) in shimmed_binary_result.envvars_override {
            log::debug!("export {:?}={:?}", k, v);
            std::env::set_var(k, v);
        }

        log::debug!("execvp {}", shimmed_binary_result.path.display());

        let mut full_args = vec![shimmed_binary_result.path.clone().into_os_string()];
        full_args.extend(args);

        Err(exec::execvp(&shimmed_binary_result.path, &full_args).into())
    } else {
        let mut unshimmed_commands =
            CheckUnshimmedCommands::new(&quickenv_home).unwrap_or(CheckUnshimmedCommands::Disabled);
        let _ignored = unshimmed_commands.exclude_current();

        let exitcode = process::Command::new(shimmed_binary_result.path)
            .args(args)
            .envs(shimmed_binary_result.envvars_override)
            .status()
            .context("failed to spawn shim subcommand")?;

        let _ignored = unshimmed_commands.check_unshimmed_commands(true);

        if let Some(code) = exitcode.code() {
            std::process::exit(code);
        }

        log::debug!("quickenv did not get an exitcode from child process, using exit 134");
        std::process::exit(134)
    }
}

struct ShimmedBinaryResult {
    path: PathBuf,
    envvars_override: core::Env,
}

fn find_shimmed_binary(
    quickenv_home: &Path,
    program_name: &OsStr,
) -> Result<ShimmedBinaryResult, Error> {
    let mut envvars_override = BTreeMap::<OsString, OsString>::new();

    if std::env::var("QUICKENV_NO_SHIM").unwrap_or_default() != "1" {
        match resolve_envrc_context(quickenv_home).and_then(|ctx| core::get_envvars(&ctx)) {
            Ok(None) => (),
            Ok(Some(envvars)) => {
                envvars_override.extend(envvars);
            }
            Err(core::Error::NoEnvrc) => (),
            Err(e) => {
                return Err(e).context("failed to get environment variables from .envrc");
            }
        }
    }

    let old_path = envvars_override
        .get(OsStr::new("PATH"))
        .cloned()
        .or_else(|| std::env::var_os("PATH"))
        .ok_or_else(|| anyhow::anyhow!("failed to read PATH"))?;
    let mut new_path = OsString::new();

    for entry in std::env::split_paths(&old_path) {
        if quickenv_home.join("bin") == entry
            || std::fs::canonicalize(&entry).map_or(false, |x| x == quickenv_home.join("bin"))
        {
            log::debug!("removing own entry from PATH: {}", entry.display());
            continue;
        }

        if !new_path.is_empty() {
            new_path.push(OsStr::new(":"));
        }

        new_path.push(entry);
    }

    envvars_override.insert(OsStr::new("PATH").to_owned(), new_path);

    let program_basename = Path::new(&program_name)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();

    let path = which::which_in(
        program_basename,
        envvars_override.get(OsStr::new("PATH")),
        std::env::current_dir().context("failed to get current working directory")?,
    )
    .with_context(|| format!("failed to find {program_basename}"))?;

    Ok(ShimmedBinaryResult {
        path,
        envvars_override,
    })
}

fn check_for_shim() -> Result<(), Error> {
    let mut args_iter = std::env::args_os();
    let program_name = args_iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("failed to determine own program name"))?;

    let program_basename = Path::new(&program_name)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();

    log::debug!("argv[0] is {:?}", program_name);

    if program_basename == "quickenv" {
        log::debug!("own program name is quickenv, so no shim running");
        return Ok(());
    }

    exec_shimmed_binary(&program_name, args_iter.collect())
        .with_context(|| format!("failed to run {}", program_basename))
}

fn command_exec(program_name: OsString, args: Vec<OsString>) -> Result<(), Error> {
    exec_shimmed_binary(&program_name, args)
}

fn command_which(program_name: OsString, pretend_shimmed: bool) -> Result<(), Error> {
    let quickenv_home = crate::core::get_quickenv_home()?;
    if !pretend_shimmed
        && which::which(&program_name)? != quickenv_home.join("bin").join(&program_name)
    {
        log::error!("{:?} is not shimmed by quickenv", program_name);
        std::process::exit(1);
    }

    let shimmed_binary_result = find_shimmed_binary(&quickenv_home, &program_name)?;
    println!("{}", shimmed_binary_result.path.display());
    Ok(())
}
