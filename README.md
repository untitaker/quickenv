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

# Into your bashrc/zshrc:
export PATH=$HOME/.quickenv/bin/:$PATH

cd ~/projects/sentry/
quickenv reload
quickenv shim sentry pytest

# these commands will now run with the virtualenv enabled
sentry devserver --workers
pytest tests/sentry/

# this one won't, whoops!
python

# better shim it!
quickenv shim python
```

## License

Licensed under `MIT`, see `LICENSE`.
