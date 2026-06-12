// generates the vault
use super::{Grid, GridType, get_grid_at, set_grid_at};
use crate::random::random_u64;
use bevy::prelude::*;

const COIN_CHANCE: f64 = 0.08;
const BRAID_DEAD_END_CHANCE: f32 = 0.75;
const ENTRANCE_ROOM_RADIUS: i32 = 2;
const EXIT_ROOM_RADIUS: i32 = 2;
const MIN_SHAFT_DISTANCE_FROM_KEY_TILES: i32 = 5;

pub fn generate(max_size: f32) -> Grid {
    generate_with_seed(max_size, random_u64())
}

pub fn generate_with_seed(max_size: f32, seed: u64) -> Grid {
    let height = sanitize_maze_size(max_size);
    let width = sanitize_maze_size(max_size * 1.5);
    let center_x = (width / 2) as f32;
    let mut rng = SeededMazeRng::new(seed);

    // Stable orientation is easier for first-time players:
    // entrance near the bottom-center, exit near the top-center.
    let entrance_pos = Vec2::new(center_x, (height.saturating_sub(2)) as f32);
    let exit_pos = Vec2::new(center_x, 1.0);

    generate_maze(width, height, entrance_pos, exit_pos, &mut rng)
}

struct SeededMazeRng {
    state: u64,
}

impl SeededMazeRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn random_unit_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / ((u64::MAX as f64) + 1.0);
        (self.next_u64() as f64) * SCALE
    }

    fn random_bool(&mut self, probability: f64) -> bool {
        debug_assert!((0.0..=1.0).contains(&probability));
        if probability <= 0.0 {
            return false;
        }
        if probability >= 1.0 {
            return true;
        }
        self.random_unit_f64() < probability
    }

    fn random_f32(&mut self) -> f32 {
        self.random_unit_f64() as f32
    }

    fn random_index(&mut self, len: usize) -> usize {
        assert!(len > 0, "cannot pick from empty slice");
        let zone = u64::MAX - (u64::MAX % len as u64);
        loop {
            let value = self.next_u64();
            if value < zone {
                return (value % len as u64) as usize;
            }
        }
    }

    fn shuffle<T>(&mut self, slice: &mut [T]) {
        if slice.len() < 2 {
            return;
        }
        for idx in (1..slice.len()).rev() {
            let swap_idx = self.random_index(idx + 1);
            slice.swap(idx, swap_idx);
        }
    }
}

fn sanitize_maze_size(input: f32) -> usize {
    let mut size = input.round() as i32;
    if size < 9 {
        size = 9;
    }
    // DFS step-2 carving is most stable on odd dimensions.
    if size % 2 == 0 {
        size += 1;
    }
    size as usize
}

pub fn find_tile(grid: &Grid, tile: u8) -> Option<IVec2> {
    for (y, row) in grid.iter().enumerate() {
        for (x, &cell) in row.iter().enumerate() {
            if cell == tile {
                return Some(IVec2::new(x as i32, y as i32));
            }
        }
    }

    None
}

fn generate_maze(
    width: usize,
    height: usize,
    entrance: Vec2,
    exit: Vec2,
    rng: &mut SeededMazeRng,
) -> Grid {
    let mut grid = vec![vec![GridType::WALL as u8; width]; height];

    fn carve(grid: &mut Grid, x: isize, y: isize, rng: &mut SeededMazeRng) {
        set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::FLOOR as u8);

        let mut dirs = vec![(0, -2), (2, 0), (0, 2), (-2, 0)];

        rng.shuffle(&mut dirs);

        for (dx, dy) in dirs {
            let nx = x + dx;
            let ny = y + dy;

            if get_grid_at(grid, nx, ny) == Some(GridType::WALL as u8) {
                // carve wall between
                set_grid_at(
                    grid,
                    Vec2::new((x + dx / 2) as f32, (y + dy / 2) as f32),
                    GridType::FLOOR as u8,
                );

                carve(grid, nx, ny, rng);
            }
        }
    }

    // start carving
    carve(&mut grid, 1, 1, rng);

    let entrance_hint = IVec2::new(entrance.x as i32, entrance.y as i32);
    let exit_hint = IVec2::new(exit.x as i32, exit.y as i32);

    let entrance_cell = clamp_interior_cell(&grid, entrance_hint).unwrap_or(IVec2::new(1, 1));
    let exit_cell =
        clamp_interior_cell(&grid, exit_hint).unwrap_or(IVec2::new((width as i32 - 2).max(1), 1));

    let entrance_connection = nearest_floor_cell(&grid, entrance_cell)
        .or_else(|| first_floor_cell(&grid))
        .unwrap_or(entrance_cell);
    let exit_connection = nearest_floor_cell(&grid, exit_cell)
        .or_else(|| farthest_floor_cell_from(&grid, entrance_cell))
        .unwrap_or(exit_cell);

    carve_room(&mut grid, entrance_cell, ENTRANCE_ROOM_RADIUS);
    carve_room(&mut grid, exit_cell, EXIT_ROOM_RADIUS);
    carve_tunnel(&mut grid, entrance_cell, entrance_connection);
    carve_tunnel(&mut grid, exit_cell, exit_connection);
    braid_dead_ends(&mut grid, BRAID_DEAD_END_CHANCE, rng);

    set_grid_at(
        &mut grid,
        Vec2::new(entrance_cell.x as f32, entrance_cell.y as f32),
        GridType::ENTRANCE as u8,
    );
    set_grid_at(
        &mut grid,
        Vec2::new(exit_cell.x as f32, exit_cell.y as f32),
        GridType::EXIT as u8,
    );
    place_hide_spots(&mut grid, rng);
    place_coins(&mut grid, rng);
    place_shaft(&mut grid, entrance_cell, exit_cell, rng);

    grid
}

fn manhattan_distance(a: IVec2, b: IVec2) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn carve_room(grid: &mut Grid, center: IVec2, radius: i32) {
    for y in (center.y - radius)..=(center.y + radius) {
        for x in (center.x - radius)..=(center.x + radius) {
            if is_interior(grid, x, y) {
                set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::FLOOR as u8);
            }
        }
    }
}

fn braid_dead_ends(grid: &mut Grid, chance: f32, rng: &mut SeededMazeRng) {
    if grid.is_empty() || grid[0].is_empty() {
        return;
    }

    let mut dead_ends = Vec::new();
    for y in 1..(grid.len() as i32 - 1) {
        for x in 1..(grid[0].len() as i32 - 1) {
            if is_dead_end(grid, x as isize, y as isize) {
                dead_ends.push((x, y));
            }
        }
    }

    rng.shuffle(&mut dead_ends);

    for (x, y) in dead_ends {
        if !is_dead_end(grid, x as isize, y as isize) {
            continue;
        }
        if rng.random_f32() > chance {
            continue;
        }

        let mut dirs = vec![(1_i32, 0_i32), (-1, 0), (0, 1), (0, -1)];
        rng.shuffle(&mut dirs);

        for (dx, dy) in dirs {
            let wall = IVec2::new(x + dx, y + dy);
            let beyond = IVec2::new(x + (dx * 2), y + (dy * 2));

            if !is_interior(grid, wall.x, wall.y) || !is_interior(grid, beyond.x, beyond.y) {
                continue;
            }
            if get_grid_at(grid, wall.x as isize, wall.y as isize) != Some(GridType::WALL as u8) {
                continue;
            }

            let beyond_tile = get_grid_at(grid, beyond.x as isize, beyond.y as isize)
                .unwrap_or(GridType::WALL as u8);
            if is_walkable(beyond_tile) {
                set_grid_at(
                    grid,
                    Vec2::new(wall.x as f32, wall.y as f32),
                    GridType::FLOOR as u8,
                );
                break;
            }
        }
    }
}

fn first_floor_cell(grid: &Grid) -> Option<IVec2> {
    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile == GridType::FLOOR as u8 && is_interior(grid, x as i32, y as i32) {
                return Some(IVec2::new(x as i32, y as i32));
            }
        }
    }
    None
}

fn farthest_floor_cell_from(grid: &Grid, from: IVec2) -> Option<IVec2> {
    let mut best: Option<(IVec2, i32)> = None;
    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile != GridType::FLOOR as u8 || !is_interior(grid, x as i32, y as i32) {
                continue;
            }
            let dx = x as i32 - from.x;
            let dy = y as i32 - from.y;
            let d2 = dx * dx + dy * dy;
            match best {
                Some((_, bd2)) if d2 <= bd2 => {}
                _ => best = Some((IVec2::new(x as i32, y as i32), d2)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn nearest_floor_cell(grid: &Grid, hint: IVec2) -> Option<IVec2> {
    let mut best: Option<(IVec2, i32)> = None;
    for (y, row) in grid.iter().enumerate() {
        for (x, &tile) in row.iter().enumerate() {
            if tile != GridType::FLOOR as u8 || !is_interior(grid, x as i32, y as i32) {
                continue;
            }
            let dx = x as i32 - hint.x;
            let dy = y as i32 - hint.y;
            let d2 = dx * dx + dy * dy;
            match best {
                Some((_, bd2)) if d2 >= bd2 => {}
                _ => best = Some((IVec2::new(x as i32, y as i32), d2)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn clamp_interior_cell(grid: &Grid, hint: IVec2) -> Option<IVec2> {
    if grid.is_empty() || grid[0].is_empty() {
        return None;
    }

    let w = grid[0].len() as i32;
    let h = grid.len() as i32;
    Some(IVec2::new(
        hint.x.clamp(1, w.saturating_sub(2)),
        hint.y.clamp(1, h.saturating_sub(2)),
    ))
}

fn carve_tunnel(grid: &mut Grid, from: IVec2, to: IVec2) {
    let mut x = from.x;
    let mut y = from.y;
    set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::FLOOR as u8);

    while x != to.x {
        x += (to.x - x).signum();
        if is_interior(grid, x, y) {
            set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::FLOOR as u8);
        }
    }

    while y != to.y {
        y += (to.y - y).signum();
        if is_interior(grid, x, y) {
            set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::FLOOR as u8);
        }
    }
}

fn is_interior(grid: &Grid, x: i32, y: i32) -> bool {
    if grid.is_empty() || grid[0].is_empty() {
        return false;
    }
    let w = grid[0].len() as i32;
    let h = grid.len() as i32;
    x > 0 && y > 0 && x < (w - 1) && y < (h - 1)
}

fn is_walkable(tile: u8) -> bool {
    tile == GridType::FLOOR as u8
        || tile == GridType::ENTRANCE as u8
        || tile == GridType::EXIT as u8
        || tile == GridType::HIDE as u8
        || tile == GridType::SHAFT as u8
}

fn place_coins(grid: &mut Grid, rng: &mut SeededMazeRng) {
    if grid.is_empty() || grid[0].is_empty() {
        return;
    }

    let height = grid.len() as isize;
    let width = grid[0].len() as isize;

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            // if the current position is a floor
            if get_grid_at(grid, x, y).unwrap_or(GridType::WALL as u8) == GridType::FLOOR as u8 {
                if rng.random_bool(COIN_CHANCE) {
                    set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::COIN as u8);
                }
            }
        }
    }
}

fn place_hide_spots(grid: &mut Grid, rng: &mut SeededMazeRng) {
    if grid.is_empty() || grid[0].is_empty() {
        return;
    }

    let height = grid.len() as isize;
    let width = grid[0].len() as isize;

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            // Dead-end floor cells become hide spots some of the time.
            if is_dead_end(grid, x, y) && rng.random_f32() < 0.40 {
                set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::HIDE as u8);
            }
        }
    }
}

fn place_shaft(grid: &mut Grid, entrance_cell: IVec2, exit_cell: IVec2, rng: &mut SeededMazeRng) {
    if grid.is_empty() || grid[0].is_empty() {
        return;
    }

    let mut dead_end_cells: Vec<(isize, isize)> = Vec::new();
    let mut floor_cells: Vec<(isize, isize)> = Vec::new();
    let height = grid.len() as isize;
    let width = grid[0].len() as isize;

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            if get_grid_at(grid, x, y).unwrap_or(GridType::WALL as u8) == GridType::FLOOR as u8 {
                floor_cells.push((x, y));
                if is_dead_end(grid, x, y) {
                    dead_end_cells.push((x, y));
                }
            }
        }
    }

    let raw_candidates = if dead_end_cells.is_empty() {
        &floor_cells
    } else {
        &dead_end_cells
    };

    let filtered: Vec<(isize, isize)> = raw_candidates
        .iter()
        .copied()
        .filter(|(x, y)| {
            let cell = IVec2::new(*x as i32, *y as i32);
            manhattan_distance(cell, entrance_cell) >= MIN_SHAFT_DISTANCE_FROM_KEY_TILES
                && manhattan_distance(cell, exit_cell) >= MIN_SHAFT_DISTANCE_FROM_KEY_TILES
        })
        .collect();

    let candidates = if filtered.is_empty() {
        raw_candidates.to_vec()
    } else {
        filtered
    };

    if candidates.is_empty() {
        return;
    }

    let idx = rng.random_index(candidates.len());
    let (x, y) = candidates[idx];
    set_grid_at(grid, Vec2::new(x as f32, y as f32), GridType::SHAFT as u8);
}

fn is_dead_end(grid: &Grid, x: isize, y: isize) -> bool {
    let tile = get_grid_at(grid, x, y).unwrap_or(GridType::WALL as u8);
    if tile != GridType::FLOOR as u8 {
        return false;
    }

    let mut exits = 0;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        if let Some(neighbor) = get_grid_at(grid, x + dx, y + dy) {
            if is_walkable(neighbor) {
                exits += 1;
            }
        }
    }

    exits == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_generation_is_repeatable() {
        let first = generate_with_seed(31.0, 12_345);
        let second = generate_with_seed(31.0, 12_345);

        assert_eq!(first, second);
    }
}
