# `quickenv`: An unintrusive environment manager

[`direnv`](https://direnv.net/) is a manager for loading/unloading environment
variables per-project. It achieves this by hooking into your shell and
executing a shellscript called `.envrc` upon `cd`, loading environment
variables generated by that shellscript into your current shell. It is useful
for automatically activating
[virtualenvs](https://docs.python.org/3/tutorial/venv.html), for example.

Unfortunately `direnv` can be a little bit "intrusive" to use. For a start, it
runs its own code in your shell. This alone is not noticeable in terms of
terminal responsiveness, but the various `.envrc`s that people end up writing
sometimes are.  `direnv` does not have a reliable, out-of-the-box way to cache
the execution of `.envrc`s, as it is arbitrary code, and so it runs everytime
you `cd` in and out of a project.

`quickenv` is a replacement for `direnv`. It works with existing `.envrc`s, and
as such is a drop-in replacement, but how you interact with `quickenv` and how
it loads environment variables is fundamentally different.

* `quickenv` does not hook into your shell. It only requires an addition to
  your `PATH`.
* `quickenv` does not load `.envrc` when changing directories. Instead you need
  to initialize `quickenv` per-project using `quickenv reload`, and rerun that
  command everytime the `.envrc` changes.
* `quickenv` does not even load environment variables into your shell. Instead
  it creates shim binaries that dispatch to the right executable.

`quickenv` is heavily inspired by [volta](https://volta.sh/) which achieves
version management for nodejs by also providing "shim" binaries for the most
common commands (`yarn`, `npm`, `node`).


## Installation

Install quickenv:

* [from GitHub](https://github.com/untitaker/quickenv/releases) as a standalone binary
* [from AUR for ArchLinux](https://aur.archlinux.org/packages/quickenv), e.g. `paru -S quickenv`
* [or build from source](#ref-build-from-source)

Then set it up in your shell:

```bash
# Into your bashrc/zshrc. This should be at the front of your PATH, such that
# quickenv can shim/shadow binaries effectively.
export PATH=$HOME/.quickenv/bin/:$PATH

# You can remove "direnv hook" from your bashrc/zshrc, but the tool needs to
# stay installed.
```

Some notes:

* `quickenv` currently assumes `direnv` is in your path, in order to load its
  "standard library".

* `quickenv` also currently does not have pre-built binaries. You need to
  [install Rust](https://rustup.rs/) and install it using Rust's package
  manager, Cargo.

* `quickenv` assumes a POSIX environment.

<a name=ref-build-from-source></a>

### Building from source

```bash
cargo install quickenv  # latest stable release
cargo install --git https://github.com/untitaker/quickenv  # latest git SHA
```

## Usage

We're going to check out [sentry](https://github.com/getsentry/sentry), because
that's one of the `.envrc`s I use. Note that Sentry's `.envrc` only works on
MacOS.

```bash
git clone https://github.com/getsentry/sentry
cd sentry

# Execute the .envrc and cache the resulting environment variables in ~/.quickenv/envs/.
# Sentry will prompt you to create a virtualenv, install dependencies via homebrew, etc.
# Re-run this command manually everytime the .envrc changes.
quickenv reload

# As part of executing the .envrc, a virtualenv has been created at './.venv/'.
# There are multiple commands available in '.venv/bin/', such as 'pytest' (a test
# runner), or 'sentry' (the main application).

# 'quickenv shim' makes those commands available in your shell.
quickenv shim

# These commands will now run with the virtualenv enabled.
sentry devserver --workers
pytest tests/sentry/
```

## Advanced usage

```bash
# Alternatively you can shim commands explicitly. Be careful: Any command you
# missed (such as 'python' or 'pip') would run outside of the virtualenv!
quickenv shim sentry pytest

# You can also run commands within the current .envrc without shimming them.
quickenv exec -- pytest

# Your git hooks don't execute in the virtualenv for some reason? Just replace
# git with a binary that itself loads the virtualenv.
quickenv shim git

# Actually activate the virtualenv in your current shell. `quickenv vars`
# prints all the extra environment variables with which each shimmed binary runs.
set -o allexport
eval "$(quickenv vars)"
set +o allexport

# Or alternatively, substitute your shell with one where your .envrc is loaded
exec quickenv exec $SHELL

# Or shim 'bash', so that when you open a subshell, the virtualenv is activated.
quickenv shim bash

# Or shim 'make', so your Makefile runs in the virtualenv.
quickenv shim make

# Curious which binary is actually being executed?
quickenv which make
# /home/user/.quickenv/bin/make

# Or for general debugging, increase the log level:
QUICKENV_LOG=debug make
# [DEBUG quickenv] argv[0] is "make"
# [DEBUG quickenv] attempting to launch shim
# [DEBUG quickenv] abspath of self is /home/user/.quickenv/bin/make
# [DEBUG quickenv] removing own entry from PATH: /home/user/.quickenv/bin
# [DEBUG quickenv] execvp /usr/bin/make
# ...
```

## License

Licensed under `MIT`, see `LICENSE`.
