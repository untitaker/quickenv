use anyhow::Error;

pub fn set_ctrlc_handler() -> Result<(), Error> {
    ctrlc::set_handler(move || {
        // necessary to work around https://github.com/mitsuhiko/dialoguer/issues/188
        let term = console::Term::stdout();
        term.show_cursor().unwrap();
    })?;
    Ok(())
}
