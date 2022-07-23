use std::fs::{create_dir_all, write};
use std::process::Command;

use anyhow::Error;
use which::which;

mod acceptance_helpers;
use acceptance_helpers::{assert_cmd, set_executable, setup};

#[test]
fn test_basic() -> Result<(), Error> {
    let harness = setup()?;
    write(harness.join(".envrc"), "export PATH=bogus:$PATH\n")?;
    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
    harness.which("hello").unwrap_err();
    assert_cmd!(harness, quickenv "shim" "hello",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] created 1 new shims in /tmp/.tmpKPvFiw/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    harness.which("hello")?;
    assert_cmd!(harness, hello,  @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] failed to check whether quickenv should run as shim
        
        Caused by:
            0: failed to find {program_basename} on path
            1: cannot find binary path
    "###);
    assert_cmd!(harness, quickenv "unshim" "hello",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] removed 1 shims from /tmp/.tmpKPvFiw/.quickenv/bin/. Use 'quickenv shim <command>' to add them again
    "###);
    which("hello").unwrap_err();

    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_shadowed() -> Result<(), Error> {
    let mut harness = setup()?;
    harness.prepend_path(harness.join("bogus"));
    create_dir_all(harness.join("bogus"))?;
    write(harness.join("bogus/hello"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/hello"))?;
    assert_cmd!(harness, hello,  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    assert_cmd!(harness, quickenv "shim" "hello",  @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] /tmp/.tmpD6ovtO/.quickenv/bin/hello is shadowed by an executable of the same name at /tmp/.tmpD6ovtO/project/bogus/hello
    "###);
    Ok(())
}

#[test]
fn test_shadowing() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "shim" "true",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] created 1 new shims in /tmp/.tmpCv6bOV/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    Ok(())
}

#[test]
fn test_shim_self() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "unshim" "quickenv",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN  quickenv] not unshimming own binary
    [INFO  quickenv] removed 0 shims from /tmp/.tmp5f4tzX/.quickenv/bin/. Use 'quickenv shim <command>' to add them again
    "###);
    assert_cmd!(harness, quickenv "shim" "quickenv",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN  quickenv] not shimming own binary
    [INFO  quickenv] created 0 new shims in /tmp/.tmp5f4tzX/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    assert_cmd!(harness, quickenv "unshim" "quickenv",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN  quickenv] not unshimming own binary
    [INFO  quickenv] removed 0 shims from /tmp/.tmp5f4tzX/.quickenv/bin/. Use 'quickenv shim <command>' to add them again
    "###);
    Ok(())
}

#[test]
fn test_verbosity() -> Result<(), Error> {
    let mut harness = setup()?;
    assert_cmd!(harness, quickenv "vars",  @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] failed to find .envrc in current or any parent directory
    "###);
    harness.set_var("QUICKENV_LOG", "debug");
    assert_cmd!(harness, quickenv "vars",  @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [DEBUG quickenv] argv[0] is "/tmp/.tmpI3TwJO/.quickenv/bin/quickenv"
    [DEBUG quickenv] own program name is quickenv, so no shim running
    [ERROR quickenv] failed to find .envrc in current or any parent directory
    "###);
    Ok(())
}

#[test]
fn test_script_failure() -> Result<(), Error> {
    let harness = setup()?;
    write(harness.join(".envrc"), "exit 1")?;
    assert_cmd!(harness, quickenv "reload",  @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] .envrc exited with status exit status: 1
    "###);
    Ok(())
}

#[test]
fn test_eating_own_tail() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "shim" "bash",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] created 1 new shims in /tmp/.tmpSkO1ZZ/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    write(
        harness.join(".envrc"),
        "bash -c 'echo hello world'; export PATH=bogus:$PATH",
    )?;
    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    create_dir_all(harness.join("bogus"))?;
    write(harness.join("bogus/hello"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/hello"))?;
    assert_cmd!(harness, quickenv "shim" "hello",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] created 1 new shims in /tmp/.tmpSkO1ZZ/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    assert_cmd!(harness, hello,  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_eating_own_tail2() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "shim" "bash",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] created 1 new shims in /tmp/.tmpgURCBM/.quickenv/bin/. Use 'quickenv unshim <command>' to remove them again
    "###);
    write(
        harness.join(".envrc"),
        "echo the value is $MYVALUE\nexport MYVALUE=canary",
    )?;
    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    the value is

    ----- stderr -----
    "###);
    // assert that during reloading, we're not shimming bash and accidentally sourcing the old
    // envvar values. canary should not appear during reload.
    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    the value is

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_exec() -> Result<(), Error> {
    let harness = setup()?;

    write(harness.join(".envrc"), "export PATH=bogus:$PATH\n")?;
    create_dir_all(harness.join("bogus"))?;
    write(harness.join("bogus/hello"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/hello"))?;

    assert_cmd!(harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [INFO  quickenv] 1 unshimmed commands. Use 'quickenv shim' to make them available.
    "###);

    harness.which("hello").unwrap_err();
    assert_cmd!(harness, quickenv "exec" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    Ok(())
}
