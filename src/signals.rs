use anyhow::Error;
use std::process::exit;

use std::sync::atomic::{AtomicBool, Ordering};

static SHIM_HAS_CONTROL: AtomicBool = AtomicBool::new(false);
const INTERRUPTED_EXIT_CODE: i32 = 130;

pub fn pass_control_to_shim() {
    // the control-passing behavior was blatantly stolen from volta.
    // https://github.com/volta-cli/volta/blob/5b5c94285500b1023f773215a7ef85aaeeeaffbd/crates/volta-core/src/signal.rs
    SHIM_HAS_CONTROL.store(true, Ordering::SeqCst);
}

pub fn set_ctrlc_handler() -> Result<(), Error> {
    ctrlc::set_handler(move || {
        if !SHIM_HAS_CONTROL.load(Ordering::SeqCst) {
            // necessary to work around https://github.com/mitsuhiko/dialoguer/issues/188
            let term = console::Term::stdout();
            term.show_cursor().unwrap();
            exit(INTERRUPTED_EXIT_CODE);
        }
    })?;
    Ok(())
}
