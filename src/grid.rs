use term_grid::{Direction, Filling, Grid, GridOptions};

pub fn print_as_grid<T: AsRef<str>>(strings: &[T]) {
    if print_as_grid_inner(strings).is_none() {
        for string in strings {
            eprintln!("{}", string.as_ref());
        }
    }
}

fn print_as_grid_inner<T: AsRef<str>>(strings: &[T]) -> Option<()> {
    let width = console::Term::stderr()
        .size_checked()
        .map(|(_rows, cols)| cols)?;

    let grid = Grid::new(
        strings.iter().collect(),
        GridOptions {
            filling: Filling::Spaces(2),
            direction: Direction::LeftToRight,
            width: width.into(),
        }
    );

    eprint!("{grid}");
    Some(())
}
