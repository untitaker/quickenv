# quickenv

**Work in progress, might eat your data**

An unintrusive replacement for direnv.

* Avoids running code on every prompt, only requires a single addition to your `PATH`.
* Avoids running the entire `.envrc` everytime you `cd` into a project, by caching the result.
* Requires you to explicitly make commands available in your shell.

## Quickstart

Note: `quickenv` currently assumes `direnv` is in your path, in order to load
its "standard library".

```bash
cargo build --release
mkdir -p ~/.quickenv/bin/
cp ./target/release/quickenv ~/.quickenv/bin/

# Into your bashrc/zshrc. This should be at the front of your PATH, such that
# quickenv can shim/shadow binaries effectively.
export PATH=$HOME/.quickenv/bin/:$PATH

# example: https://github.com/getsentry/sentry
# Note: Sentry's .envrc only works on MacOS
cd ~/projects/sentry/

# Execute the .envrc and cache the resulting environment variables in ~/.quickenv/envs/.
# quickenv won't do any cache invalidation for you (for now). Use your own
# judgement to see whether changes in .envrc are relevant to you.
quickenv reload

quickenv shim sentry pytest

# These commands will now run with the virtualenv enabled
sentry devserver --workers
pytest tests/sentry/

# Other commands not explicitly shimmed will end up not running in the virtualenv at all.
python
pip install ...

# Better shim it!
quickenv shim python pip
```

## Advanced usage

```bash
# Your git hooks don't execute in the virtualenv for some reason? Just replace
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
