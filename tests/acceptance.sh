#!/bin/bash

set -e
set -o pipefail

export C_GREEN="\e[0;32m"
export C_RED="\e[0;31m"
export C_RESET="\e[0m"

cargo build
cargo_root=$PWD

TESTFILTER=$1

setup() {
    TEMPDIR="$(mktemp -d)"
    cd "$TEMPDIR"
    export QUICKENV_HOME="$TEMPDIR"
    mkdir bin
    ln -s "$cargo_root/target/debug/quickenv" ./bin/quickenv
    old_path="$PATH"
    export PATH="$QUICKENV_HOME/bin:$PATH"
    mkdir project
    cd project
}

teardown() {
    rm -rf "$TEMPDIR"
}

testcase() {
    if [ -n "$TESTFILTER" ] && [ "$1" != "$TESTFILTER" ]; then
        return
    fi

    testfunc="$1"
    echo -n "test $testfunc... "
    setup
    set +e
    output="$(testcase_inner $testfunc 2>&1)"
    exitcode=$?
    set -e

    if [ $exitcode != 0 ]; then
        echo -e "${C_RED}FAILED:${C_RESET}"
        echo "$output"
        exit 1
    fi
    teardown
    echo -e "${C_GREEN}SUCCESS${C_RESET}"
}

testcase_inner() (
    set -xe
    set -o pipefail
    $1
)

test_basic() {
    echo 'export PATH=bogus:$PATH' > .envrc
    output="$(quickenv reload 2>&1)"
    echo "$output" | grep "new PATH entry: bogus"
    mkdir bogus
    echo 'echo hello world' > bogus/hello
    chmod +x bogus/hello
    ! which hello
    quickenv shim hello
    which hello
    [ "$(QUICKENV_LOG=debug hello)" = "hello world" ]
    quickenv unshim hello
    ! which hello

    output="$(quickenv vars 2>&1)"
    echo "$output" | grep 'PATH=bogus:'

    output="$(quickenv reload)"
    ! (echo "$output" | grep "new PATH entry:")
}

testcase test_basic

test_shadowing() {
    output="$(quickenv shim true 2>&1)"
    echo "$output" | grep -E 'shadowing binary at /.+/true'
}

testcase test_shadowing

test_shadowed() {
    export PATH="bogus:$PATH"
    mkdir bogus
    echo 'echo hello world' > bogus/hello
    chmod +x bogus/hello
    [ "$(hello)" = "hello world" ]
    output="$(! quickenv shim hello 2>&1)"
    echo "$output" | grep "$QUICKENV_HOME/bin/hello is shadowed by an executable of the same name at bogus/hello"
}

testcase test_shadowed

test_shim_self() {
    output="$(quickenv unshim quickenv 2>&1)"
    echo "$output" | grep -E 'removed 0 shims'
    echo "$output" | grep 'not unshimming own binary'
    output="$(quickenv shim quickenv 2>&1)"
    echo "$output" | grep -E 'created 0 new shims'
    echo "$output" | grep 'not shimming own binary'
    output="$(quickenv unshim quickenv 2>&1)"
    echo "$output" | grep -E 'removed 0 shims'
    echo "$output" | grep 'not unshimming own binary'
}

testcase test_shim_self

test_verbosity() {
    ! (quickenv vars 2>&1 || true) | grep 'own program name is quickenv, so no shim running'
    export QUICKENV_LOG=debug
    (quickenv vars 2>&1 || true) | grep 'own program name is quickenv, so no shim running'
}

testcase test_verbosity

test_script_failure() {
    echo 'exit 1' > .envrc
    ! quickenv reload
}

testcase test_script_failure

test_eating_own_tail() {
    quickenv shim bash
    echo 'bash -c "echo hello world"; export PATH=bogus:$PATH' > .envrc
    QUICKENV_LOG=debug timeout 1 quickenv reload
    mkdir bogus
    echo 'echo hello world' > bogus/hello
    chmod +x bogus/hello
    quickenv shim hello
    hello
}

testcase test_eating_own_tail

test_eating_own_tail2() {
    quickenv shim bash
    echo 'echo the value is $MYVALUE' > .envrc
    echo 'bash -c "echo the value is $MYVALUE"' > .envrc
    echo 'export MYVALUE=canary' >> .envrc
    output="$(quickenv reload)"
    ! echo "$output" | grep canary
    # Assert that during reloading, we're not shimming bash and accidentally
    # sourcing the old envvar values
    output="$(quickenv reload)"
    ! echo "$output" | grep canary
}

testcase test_eating_own_tail2
