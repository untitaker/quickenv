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
  you tell `quickenv` which binaries should run with those environment
  variables present (for example `quickenv shim python pytest poetry`), and
  quickenv will wrap those commands in a custom binary, and put that "shim" on
  your `PATH`.

`quickenv` is heavily inspired by [volta](https://volta.sh/) which achieves
version management for nodejs by also providing "shim" binaries for the most
common commands (`yarn`, `npm`, `node`).


## Installation

**quickenv is work in progress and most likely contains bugs. that said, I use it daily at work**

* `quickenv` currently assumes `direnv` is in your path, in order to load
its "standard library".

* `quickenv` also currently does not have pre-built binaries. You need to
[install Rust](https://rustup.rs/), check out this repository, and install it
yourself.

* `quickenv` assumes a POSIX environment.

```bash
cargo install quickenv

# Into your bashrc/zshrc. This should be at the front of your PATH, such that
# quickenv can shim/shadow binaries effectively.
export PATH=$HOME/.quickenv/bin/:$PATH
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

# Tell quickenv to place "shim" binaries for those commands in ~/.quickenv/bin/
quickenv shim sentry pytest

# These commands will now run with the virtualenv enabled
sentry devserver --workers
pytest tests/sentry/

# Other commands not explicitly shimmed will end up not running in the
# virtualenv at all. Whoops!
python
pip install ...

# Better shim them!
quickenv shim python pip
```

## Advanced usage

```bash
# Your git hooks don't execute in the virtualenv for some reason? Just replace/shadow
# git with a binary that itself loads the virtualenv.
quickenv shim git

# Actually activate the virtualenv in your current shell. `quickenv vars`
# prints all the extra environment variables with which each shimmed binary runs.
eval "$(quickenv vars)"

# Or shim 'bash', so that when you open a subshell, the virtualenv is activated.
quickenv shim bash

# Or shim 'make', so your Makefile runs in the virtualenv. This can save you from
# explicitly enumerating a bunch of commands, if you only ever run them via 'make'.
quickenv shim make
```

## License

Licensed under `MIT`, see `LICENSE`.
