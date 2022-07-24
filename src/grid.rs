use term_grid::{Cell, Direction, Filling, Grid, GridOptions};

pub fn print_as_grid<T: AsRef<str>>(strings: &[T]) {
    if print_as_grid_inner(strings).is_none() {
        for string in strings {
            println!("{}", string.as_ref());
        }
    }
}

fn print_as_grid_inner<T: AsRef<str>>(strings: &[T]) -> Option<()> {
    let mut grid = Grid::new(GridOptions {
        filling: Filling::Spaces(2),
        direction: Direction::LeftToRight,
    });

    for string in strings {
        grid.add(Cell {
            contents: string.as_ref().into(),
            width: console::measure_text_width(string.as_ref()),
        });
    }

    let width = term_size::dimensions().map(|(w, _h)| w)?;
    let grid = grid.fit_into_width(width)?;

    print!("{grid}");
    Some(())
}
