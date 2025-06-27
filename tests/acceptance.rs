use std::fs::{create_dir_all, write};

use anyhow::Error;
use which::which;

mod acceptance_helpers;
use acceptance_helpers::{assert_cmd, set_executable, setup};

#[test]
fn test_basic() -> Result<(), Error> {
    let harness = setup()?;
    write(harness.join(".envrc"), "export PATH=bogus:$PATH\n")?;
    create_dir_all(harness.join("bogus"))?;
    write(harness.join("bogus/hello"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/hello"))?;

    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);
    harness.which("hello").unwrap_err();
    assert_cmd!(harness, quickenv "shim" "hello",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);
    harness.which("hello")?;
    assert_cmd!(harness, hello,  @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    assert_cmd!(harness, quickenv "unshim" "hello",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Removed 1 shims from [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv shim <command>' to add them again
    "###);
    which("hello").unwrap_err();

    assert_cmd!(harness, quickenv "reload",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] 1 unshimmed commands. Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
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
    [ERROR quickenv] [scrubbed $HOME]/.quickenv/bin/hello is shadowed by an executable of the same name at [scrubbed $HOME]/project/bogus/hello
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
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
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
    [WARN quickenv] not unshimming own binary
    Removed 0 shims from [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv shim <command>' to add them again
    "###);
    assert_cmd!(harness, quickenv "shim" "quickenv",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] not shimming own binary
    created no new shims.
    "###);
    assert_cmd!(harness, quickenv "unshim" "quickenv",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] not unshimming own binary
    Removed 0 shims from [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv shim <command>' to add them again
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
    [DEBUG quickenv] argv[0] is "[scrubbed $HOME]/.quickenv/quickenv_bin/quickenv"
    [DEBUG quickenv] own program name is quickenv, so no shim running
    [ERROR quickenv] failed to find .envrc in current or any parent directory
    "###);
    Ok(())
}

#[test]
fn test_vars_quiet() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "vars" "-q",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
    assert_cmd!(harness, quickenv "vars" "--quiet",  @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
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
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
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
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
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
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
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
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
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

#[test]
fn test_shim_creating_shims() -> Result<(), Error> {
    let harness = setup()?;

    write(harness.join(".envrc"), "export PATH=bogus:$PATH\n")?;
    assert_cmd!(harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    create_dir_all(harness.join("bogus"))?;
    write(harness.join("bogus/hello"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/hello"))?;

    // there is a command. it does not create more commands. quickenv should not amend any output
    assert_cmd!(harness, quickenv "exec" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);

    // shimming the command should work
    assert_cmd!(harness, quickenv "shim" "--yes", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found these unshimmed commands in your .envrc:

    hello

    Quickenv will create this new shim binary in [scrubbed $HOME]/.quickenv/bin/.
    Inside of [scrubbed $HOME]/project, those commands will run with .envrc enabled.
    Outside, they will run normally.
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    Use 'quickenv shim <command>' to run additional commands with .envrc enabled.
    "###);

    // change the command such that it creates another command, and run it
    write(
        harness.join("bogus/hello"),
        "#!/bin/sh\necho 'echo hello world' > bogus/hello2 && chmod +x bogus/hello2",
    )?;
    set_executable(harness.join("bogus/hello"))?;

    // quickenv should warn that more commands need shimming now
    assert_cmd!(harness, quickenv "exec" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);

    // quickenv shim should find the new command
    assert_cmd!(harness, quickenv "shim" "--yes", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found these unshimmed commands in your .envrc:

    hello2

    Quickenv will create this new shim binary in [scrubbed $HOME]/.quickenv/bin/.
    Inside of [scrubbed $HOME]/project, those commands will run with .envrc enabled.
    Outside, they will run normally.
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    Use 'quickenv shim <command>' to run additional commands with .envrc enabled.
    "###);

    Ok(())
}

#[test]
fn test_auto_shimming() -> Result<(), Error> {
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
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);

    assert_cmd!(harness, quickenv "shim" "-y", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Found these unshimmed commands in your .envrc:

    hello

    Quickenv will create this new shim binary in [scrubbed $HOME]/.quickenv/bin/.
    Inside of [scrubbed $HOME]/project, those commands will run with .envrc enabled.
    Outside, they will run normally.
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    Use 'quickenv shim <command>' to run additional commands with .envrc enabled.
    "###);

    assert_cmd!(harness, quickenv "shim" "-y", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    created no new shims.
    Use 'quickenv shim <command>' to run additional commands with .envrc enabled.
    "###);

    Ok(())
}

#[test]
fn test_no_envrc_context() -> Result<(), Error> {
    let harness = setup()?;
    assert_cmd!(harness, quickenv "shim" "echo", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);
    assert_cmd!(harness, echo "hello world", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello world

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_eating_own_tail3() -> Result<(), Error> {
    // regression: we removed our own PATH from the PATH envvar, but:
    // 1) the path is actually duplicated, so we didn't remove all copies, and we recurse into
    //    the shim
    // 2) the shim re-adds the PATH through its envrc cache
    let mut harness = setup()?;
    harness.prepend_path(std::fs::canonicalize(harness.join("../.quickenv/bin")).unwrap());
    write(harness.join(".envrc"), "export PATH=hello:$PATH:")?;
    assert_cmd!(harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);
    assert_cmd!(harness, quickenv "shim" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);
    harness.set_var("QUICKENV_LOG", "debug");
    assert_cmd!(harness, hello, @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [DEBUG quickenv] argv[0] is "[scrubbed $HOME]/.quickenv/bin/hello"
    [DEBUG quickenv] attempting to launch shim for "[scrubbed $HOME]/.quickenv/bin/hello"
    [DEBUG quickenv] loading [scrubbed $HOME]/project/.envrc
    [DEBUG quickenv] removing own entry from PATH: [scrubbed $HOME]/.quickenv/bin
    [DEBUG quickenv] removing own entry from PATH: [scrubbed $HOME]/.quickenv/bin
    [ERROR quickenv] failed to run shimmed command

    Caused by:
        0: failed to run hello
        1: failed to find actual binary
        2: failed to find hello
        3: cannot find binary path
    "###);
    Ok(())
}

#[test]
fn test_which() -> Result<(), Error> {
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
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);
    assert_cmd!(harness, quickenv "which" "hello", @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] cannot find binary path
    "###);
    assert_cmd!(harness, quickenv "shim" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);
    assert_cmd!(harness, quickenv "which" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    bogus/hello

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_which_pretend_shimmed() -> Result<(), Error> {
    let harness = setup()?;

    write(harness.join(".envrc"), "export PATH=bogus:$PATH\n")?;
    create_dir_all(harness.join("bogus"))?;

    assert_cmd!(harness, quickenv "which" "bash", @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] "bash" is not shimmed by quickenv
    "###);

    assert_cmd!(harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    "###);

    assert_cmd!(harness, quickenv "which" "bash", @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] "bash" is not shimmed by quickenv
    "###);

    write(harness.join("bogus/bash"), "#!/bin/sh\necho hello world")?;
    set_executable(harness.join("bogus/bash"))?;

    assert_cmd!(harness, quickenv "which" "bash", @r###"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    [ERROR quickenv] "bash" is not shimmed by quickenv
    "###);

    assert_cmd!(harness, quickenv "which" "bash" "--pretend-shimmed", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    bogus/bash

    ----- stderr -----
    "###);
    Ok(())
}

#[test]
fn test_shim_ctrl_c_handling() -> Result<(), Error> {
    let harness = setup()?;

    // Create a test script that sets up a signal handler
    write(harness.join(".envrc"), "export PATH=test_bin:$PATH\n")?;
    create_dir_all(harness.join("test_bin"))?;

    // Create a test script that demonstrates the signal handling behavior
    write(
        harness.join("test_bin/signal_test"),
        r#"#!/bin/sh
myexit() { 
    echo "shutdown"
    sleep 0.1
    echo "bye"
}
trap myexit SIGINT
sleep 1
"#,
    )?;
    set_executable(harness.join("test_bin/signal_test"))?;

    assert_cmd!(harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);

    assert_cmd!(harness, quickenv "shim" "signal_test", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);

    // Test that the shim properly handles SIGINT by using timeout
    // This reproduces the exact issue from GitHub issue #12
    let harness_modified = harness;

    // Use the same repro case as in the issue
    let timeout_cmd = format!(
        "timeout -s SIGINT 0.5 {} exec sh -c 'myexit() {{ echo shutdown; sleep 1; echo bye; }}; trap myexit INT; sleep 10'; echo new_prompt",
        harness_modified.which("quickenv")?.display()
    );

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&timeout_cmd)
        .current_dir(&harness_modified.cwd)
        .envs(&harness_modified.env)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // verify that the signal was handled properly. we should see:
    // 1. "shutdown"
    // 2. "bye"
    // 3. "new_prompt"
    assert!(
        stdout.contains("shutdown"),
        "Expected 'shutdown' in output: {stdout}"
    );
    assert!(stdout.contains("bye"), "Expected 'bye' in output: {stdout}");
    assert!(
        stdout.contains("new_prompt"),
        "Expected 'new_prompt' in output: {stdout}"
    );

    // "bye" must come before "new_prompt"
    // If signal handling is not done correctly, quickenv exits immediately and "new_prompt"
    // appears before "bye"
    let bye_pos = stdout.find("bye").expect("Expected 'bye' in output");
    let new_prompt_pos = stdout
        .find("new_prompt")
        .expect("Expected 'new_prompt' in output");
    assert!(bye_pos < new_prompt_pos,
        "CTRL-C handling bug detected: 'bye' should come before 'new_prompt'. Output order indicates quickenv exited early and orphaned the child process. Actual output: {stdout}");

    Ok(())
}

#[test]
fn test_direnv_lib_loading() -> Result<(), Error> {
    let harness = setup()?;

    // Create a fake direnv lib directory and file
    let direnv_lib_dir = harness.join(".config/direnv/lib");
    create_dir_all(&direnv_lib_dir)?;

    // Create a lib file that exports a custom function and variable
    write(
        direnv_lib_dir.join("test_lib.sh"),
        r#"#!/bin/bash
# Test direnv lib file
export DIRENV_LIB_LOADED=true
my_custom_function() {
    echo "custom function from direnv lib"
}
"#,
    )?;

    // Create .envrc that uses the function and variable from the lib
    write(
        harness.join(".envrc"),
        r#"my_custom_function
echo "DIRENV_LIB_LOADED is: $DIRENV_LIB_LOADED"
export PATH=test_bin:$PATH
"#,
    )?;

    // Create a test binary
    create_dir_all(harness.join("test_bin"))?;
    write(
        harness.join("test_bin/hello"),
        "#!/bin/sh\necho hello from test bin",
    )?;
    set_executable(harness.join("test_bin/hello"))?;

    // Set XDG_CONFIG_HOME to point to our test config
    let mut test_harness = harness;
    test_harness.set_var("XDG_CONFIG_HOME", test_harness.join(".config"));

    // Test that lib loading works during reload
    assert_cmd!(test_harness, quickenv "reload", @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    custom function from direnv lib
    DIRENV_LIB_LOADED is: true

    ----- stderr -----
    [WARN quickenv] 1 unshimmed commands (1 new). Use 'quickenv shim' to make them available.
    Set QUICKENV_NO_SHIM_WARNINGS=1 to silence this message.
    "###);

    // Test that lib loading works when executing shimmed commands
    assert_cmd!(test_harness, quickenv "shim" "hello", @r###"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    Created 1 new shims in [scrubbed $HOME]/.quickenv/bin/.
    Use 'quickenv unshim <command>' to remove them again.
    "###);

    // The environment should be loaded properly when running the shimmed command
    assert_cmd!(test_harness, hello, @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    hello from test bin

    ----- stderr -----
    "###);

    Ok(())
}
