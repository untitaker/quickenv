use std::collections::{BTreeMap, BTreeSet};

use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, BufWriter, Write};

use std::os::unix::fs::symlink;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};

use log::LevelFilter;

use anyhow::{Context, Error};
use clap::Parser;

mod core;

#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
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
        #[clap(long)]
        yes: bool,
        /// The names of the commands to expose. If missing, quickenv will determine recommended
        /// commands itself and ask for confirmation.
        #[clap(value_parser)]
        commands: Vec<String>,
    },
    /// Remove a shim binary from ~/.quickenv/bin/.
    Unshim {
        #[clap(value_parser)]
        /// The names of the commands to remove.
        commands: Vec<String>,
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
        .format_timestamp(None)
        .filter_level(LevelFilter::Info)
        .parse_env("QUICKENV_LOG")
        .init();

    check_for_shim().context("failed to check whether quickenv should run as shim")?;

    let args = Args::parse();

    match args.subcommand {
        Command::Reload => command_reload(),
        Command::Vars => command_vars(),
        Command::Shim { commands, yes } => command_shim(commands, yes),
        Command::Unshim { commands } => command_unshim(commands),
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

fn parse_env_line(line: &str, env: &mut core::Env, prev_var_name: &mut Option<String>) {
    match line.split_once('=') {
        Some((var_name, value)) => {
            env.insert(var_name.to_owned(), value.to_owned());
            *prev_var_name = Some(var_name.to_owned());
        }
        None => {
            let prev_value = env
                .get_mut(prev_var_name.as_ref().unwrap().as_str())
                .unwrap();
            prev_value.push('\n');
            prev_value.push_str(line);
        }
    }
}

fn parse_env_diff<R: BufRead>(
    reader: R,
    mut script_output: impl FnMut(&str),
) -> Result<(core::Env, core::Env), Error> {
    let mut parse_state = ParseState::PreBefore;
    let mut old_env = BTreeMap::new();
    let mut new_env = BTreeMap::new();
    let mut prev_var_name = None;

    for raw_line in reader.lines() {
        let raw_line = raw_line?;
        let line = raw_line.trim_end_matches('\n');
        match (parse_state, line) {
            (ParseState::PreBefore, "// BEGIN QUICKENV-BEFORE") => {
                prev_var_name = None;
                parse_state = ParseState::InBefore;
            }
            (ParseState::InBefore, "// END QUICKENV-BEFORE") => {
                prev_var_name = None;
                parse_state = ParseState::PreAfter;
            }
            (ParseState::PreAfter, "// BEGIN QUICKENV-AFTER") => {
                prev_var_name = None;
                parse_state = ParseState::InAfter;
            }
            (ParseState::InAfter, "// END QUICKENV-AFTER") => {
                prev_var_name = None;
                parse_state = ParseState::End;
            }
            (ParseState::InBefore, line) => {
                parse_env_line(line, &mut old_env, &mut prev_var_name);
            }
            (ParseState::InAfter, line) => {
                parse_env_line(line, &mut new_env, &mut prev_var_name);
            }
            (_, _) => {
                script_output(&raw_line);
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

    let mut output = Vec::new();
    let (old_env, new_env) =
        parse_env_diff(input.as_slice(), |line| output.push(line.to_owned())).unwrap();
    assert_eq!(
        old_env,
        maplit::btreemap![
            "hello".to_owned() => "world".to_owned(),
            "bogus".to_owned() => "wogus".to_owned(),
        ]
    );

    assert_eq!(
        new_env,
        maplit::btreemap![
            "hello".to_owned() => "world".to_owned(),
            "bogus".to_owned() => "wogus\n2".to_owned(),
            "more".to_owned() => "keys".to_owned(),
        ]
    );

    assert_eq!(
        output,
        vec![
            "".to_owned(),
            "some output 1".to_owned(),
            "some output 2".to_owned(),
            "some output 3".to_owned()
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

    temp_script
        .write_all(
            br#"
echo '// BEGIN QUICKENV-BEFORE'
env
echo '// END QUICKENV-BEFORE'
eval "$(direnv stdlib)"
"#,
        )
        .with_context(|| {
            format!(
                "failed to write to temporary file at {}",
                temp_script.path().display()
            )
        })?;

    io::copy(&mut ctx.envrc, &mut temp_script).with_context(|| {
        format!(
            "failed to write to temporary file at {}",
            temp_script.path().display()
        )
    })?;

    temp_script
        .write_all(
            br#"
echo '// BEGIN QUICKENV-AFTER'
env
echo '// END QUICKENV-AFTER'
"#,
        )
        .with_context(|| {
            format!(
                "failed to write to temporary file at {}",
                temp_script.path().display()
            )
        })?;

    let mut cmd = process::Command::new("bash")
        .arg(temp_script.path())
        .env("QUICKENV_NO_SHIM", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .current_dir(ctx.root)
        .spawn()
        .context("failed to spawn bash for running envrc")?;

    let stdout_buf = BufReader::new(cmd.stdout.take().unwrap());
    let (old_env, new_env) = parse_env_diff(stdout_buf, |line| println!("{line}"))
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
            writeln!(&mut env_cache, "{key}={value}").with_context(|| {
                format!(
                    "failed to write to envrc cache at {}",
                    ctx.env_cache_path.display()
                )
            })?;
        }
    }

    Ok(())
}

fn get_missing_shims(
    quickenv_home: &Path,
    new_path_envvar: Option<&str>,
) -> Result<Vec<String>, Error> {
    let mut rv = Vec::new();
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
    rv: &mut Vec<String>,
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

        if !quickenv_home
            .join("bin")
            .join(filename)
            .try_exists()
            .unwrap_or(false)
        {
            rv.push(filename.to_owned());
        }
    }

    Ok(())
}

fn command_reload() -> Result<(), Error> {
    let quickenv_dir = crate::core::get_quickenv_home()?;
    compute_envvars(&quickenv_dir)?;
    let new_envvars =
        crate::core::get_envvars(&quickenv_dir)?.expect("somehow didn't end up writing envvars");
    let new_path_envvar = new_envvars.get("PATH").map(String::as_str);
    let missing_shims = get_missing_shims(&quickenv_dir, new_path_envvar)?;

    if !missing_shims.is_empty() {
        log::info!(
            "{} unshimmed commands. Use 'quickenv shim' to make them available.",
            missing_shims.len()
        )
    }

    Ok(())
}

fn command_vars() -> Result<(), Error> {
    let quickenv_dir = crate::core::get_quickenv_home()?;
    if let Some(envvars) = core::get_envvars(&quickenv_dir)? {
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

fn command_shim(mut commands: Vec<String>, yes: bool) -> Result<(), Error> {
    let quickenv_dir = crate::core::get_quickenv_home()?;
    let bin_dir = quickenv_dir.join("bin/");

    if commands.is_empty() {
        let envvars = crate::core::get_envvars(&quickenv_dir)?
            .ok_or_else(|| anyhow::anyhow!("run 'quickenv reload' first to generate envvars"))?;
        let path_envvar = envvars.get("PATH").map(String::as_str);
        commands = get_missing_shims(&quickenv_dir, path_envvar)?;

        if !yes {
            log::info!("creating the following shims from new PATH entries:");
            for command in &commands {
                log::info!("  {command}");
            }

            let prompt = format!(
                "do you want to continue with creating {} new shim binaries in {}?",
                commands.len(),
                bin_dir.display()
            );

            let answer = inquire::Confirm::new(&prompt)
                .with_default(true)
                .with_help_message("you will still be able to use those commands outside of your .envrc environment")
                .prompt()?;

            if !answer {
                std::process::exit(1);
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
            Err(anyhow::anyhow!(
                "{} is shadowed by an executable of the same name at {}",
                command_path.display(),
                effective_command_path.display(),
            ))?
        }
    }

    log::info!(
        "created {} new shims in {}. Use 'quickenv unshim <command>' to remove them again",
        changes,
        bin_dir.display()
    );

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
        "removed {} shims from {}. Use 'quickenv shim <command>' to add them again",
        changes,
        bin_dir.display()
    );

    Ok(())
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

    log::debug!("attempting to launch shim");

    let own_path =
        which::which(&program_name).context("failed to determine path of own program")?;
    log::debug!("abspath of self is {}", own_path.display());
    let own_path_parent = match own_path.parent() {
        Some(x) => x,
        None => {
            return Err(anyhow::anyhow!(
                "own path has no parent directory: {}",
                own_path.display()
            ));
        }
    };

    if std::env::var("QUICKENV_NO_SHIM").unwrap_or_default() != "1" {
        let quickenv_dir = crate::core::get_quickenv_home()?;
        match core::get_envvars(&quickenv_dir) {
            Ok(None) => (),
            Ok(Some(envvars)) => {
                for (k, v) in envvars {
                    std::env::set_var(k, v);
                }
            }
            Err(core::Error::NoEnvrc) => (),
            Err(e) => {
                return Err(e).context("failed to get environment variables from .envrc");
            }
        }
    }

    let mut new_path = OsString::new();

    let mut deleted_own_path = false;

    for entry in std::env::split_paths(&std::env::var("PATH").context("failed to read PATH")?) {
        if !deleted_own_path
            && (own_path_parent == entry
                || std::fs::canonicalize(&entry).map_or(false, |x| x == own_path_parent))
        {
            log::debug!("removing own entry from PATH: {}", entry.display());
            deleted_own_path = true;
            continue;
        }

        if !new_path.is_empty() {
            new_path.push(OsStr::new(":"));
        }

        new_path.push(entry);
    }

    std::env::set_var("PATH", new_path);

    let path =
        which::which(&program_basename).context("failed to find {program_basename} on path")?;
    log::debug!("execvp {}", path.display());

    let mut args = vec![path.into_os_string()];
    args.extend(args_iter);

    Err(exec::execvp(&args[0], &args).into())
}
