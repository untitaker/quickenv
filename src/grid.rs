use term_grid::{Direction, Filling, Grid, GridOptions};

pub fn print_as_grid<T: AsRef<str>>(strings: &[T]) {
    if print_as_grid_inner(strings).is_none() {
        for string in strings {
            eprintln!("{}", string.as_ref());
        }
    }
}

fn print_as_grid_inner<T: AsRef<str>>(strings: &[T]) -> Option<()> {
    let mut grid = Grid::new(GridOptions {
        filling: Filling::Spaces(2),
        direction: Direction::LeftToRight,
    });

    for string in strings {
        grid.add(string.as_ref().into());
    }

    let width = console::Term::stderr().size_checked().map(|(w, _h)| w)?;
    let grid = grid.fit_into_width(width.into())?;

    eprint!("{grid}");
    Some(())
}
