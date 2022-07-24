use anyhow::Error;

pub fn set_ctrlc_handler() -> Result<(), Error> {
    ctrlc::set_handler(move || {
        let term = console::Term::stdout();
        term.show_cursor().unwrap();
    })?;
    Ok(())
}
