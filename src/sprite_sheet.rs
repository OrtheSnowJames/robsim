use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Down,
    Up,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct FacingColumns {
    pub down: usize,
    pub up: usize,
    pub left: usize,
    pub right: usize,
}

impl FacingColumns {
    pub fn new(down: usize, up: usize, left: usize, right: usize) -> Self {
        Self {
            down,
            up,
            left,
            right,
        }
    }

    pub fn get(self, facing: Facing) -> usize {
        match facing {
            Facing::Down => self.down,
            Facing::Up => self.up,
            Facing::Left => self.left,
            Facing::Right => self.right,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FacingRows {
    pub down: usize,
    pub up: usize,
    pub left: usize,
    pub right: usize,
}

impl FacingRows {
    pub fn new(down: usize, up: usize, left: usize, right: usize) -> Self {
        Self {
            down,
            up,
            left,
            right,
        }
    }

    pub fn get(self, facing: Facing) -> usize {
        match facing {
            Facing::Down => self.down,
            Facing::Up => self.up,
            Facing::Left => self.left,
            Facing::Right => self.right,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SpriteSheetMapping {
    DirectionByColumn {
        facing_columns: FacingColumns,
        idle_row: usize,
        walk_start_row: usize,
        walk_frames: usize,
    },
    DirectionByRow {
        facing_rows: FacingRows,
        idle_col: usize,
        walk_start_col: usize,
        walk_frames: usize,
    },
}

#[derive(Clone, Debug)]
pub struct SpriteSheetConfig {
    pub tile_size: UVec2,
    pub atlas_columns: usize,
    pub atlas_rows: usize,
    pub mapping: SpriteSheetMapping,
    pub frame_time_secs: f32,
}

impl SpriteSheetConfig {
    pub fn from_grid_direction_by_row(
        tile_size: UVec2,
        atlas_columns: usize,
        atlas_rows: usize,
        facing_rows: FacingRows,
        idle_col: usize,
        walk_start_col: usize,
        walk_frames: usize,
        frame_time_secs: f32,
    ) -> Self {
        Self {
            tile_size,
            atlas_columns,
            atlas_rows,
            mapping: SpriteSheetMapping::DirectionByRow {
                facing_rows,
                idle_col,
                walk_start_col,
                walk_frames,
            },
            frame_time_secs,
        }
    }

    pub fn from_grid_direction_by_column(
        tile_size: UVec2,
        atlas_columns: usize,
        atlas_rows: usize,
        facing_columns: FacingColumns,
        idle_row: usize,
        walk_start_row: usize,
        walk_frames: usize,
        frame_time_secs: f32,
    ) -> Self {
        Self {
            tile_size,
            atlas_columns,
            atlas_rows,
            mapping: SpriteSheetMapping::DirectionByColumn {
                facing_columns,
                idle_row,
                walk_start_row,
                walk_frames,
            },
            frame_time_secs,
        }
    }

    pub fn simple_4dir_rows(
        tile_px: u32,
        atlas_columns: usize,
        atlas_rows: usize,
        down_row: usize,
        up_row: usize,
        side_row: usize,
        frame_time_secs: f32,
    ) -> Self {
        Self::from_grid_direction_by_row(
            UVec2::splat(tile_px),
            atlas_columns,
            atlas_rows,
            FacingRows::new(down_row, up_row, side_row, side_row),
            0,
            1,
            2,
            frame_time_secs,
        )
    }

    pub fn validate(&self) {
        assert!(self.atlas_columns > 0, "atlas_columns must be > 0");
        assert!(self.atlas_rows > 0, "atlas_rows must be > 0");
        assert!(self.tile_size.x > 0 && self.tile_size.y > 0, "tile_size must be > 0");
        match self.mapping {
            SpriteSheetMapping::DirectionByColumn {
                facing_columns,
                idle_row,
                walk_start_row,
                walk_frames,
            } => {
                assert!(walk_frames > 0, "walk_frames must be > 0");
                assert!(idle_row < self.atlas_rows, "idle_row out of bounds");
                assert!(walk_start_row < self.atlas_rows, "walk_start_row out of bounds");
                assert!(
                    walk_start_row + walk_frames - 1 < self.atlas_rows,
                    "walk rows out of bounds"
                );
                assert!(facing_columns.down < self.atlas_columns, "down column out of bounds");
                assert!(facing_columns.up < self.atlas_columns, "up column out of bounds");
                assert!(facing_columns.left < self.atlas_columns, "left column out of bounds");
                assert!(facing_columns.right < self.atlas_columns, "right column out of bounds");
            }
            SpriteSheetMapping::DirectionByRow {
                facing_rows,
                idle_col,
                walk_start_col,
                walk_frames,
            } => {
                assert!(walk_frames > 0, "walk_frames must be > 0");
                assert!(idle_col < self.atlas_columns, "idle_col out of bounds");
                assert!(walk_start_col < self.atlas_columns, "walk_start_col out of bounds");
                assert!(
                    walk_start_col + walk_frames - 1 < self.atlas_columns,
                    "walk cols out of bounds"
                );
                assert!(facing_rows.down < self.atlas_rows, "down row out of bounds");
                assert!(facing_rows.up < self.atlas_rows, "up row out of bounds");
                assert!(facing_rows.left < self.atlas_rows, "left row out of bounds");
                assert!(facing_rows.right < self.atlas_rows, "right row out of bounds");
            }
        }
    }

    pub fn layout(&self, layouts: &mut Assets<TextureAtlasLayout>) -> Handle<TextureAtlasLayout> {
        self.validate();
        layouts.add(TextureAtlasLayout::from_grid(
            self.tile_size,
            self.atlas_columns as u32,
            self.atlas_rows as u32,
            None,
            None,
        ))
    }

    pub fn atlas_index(&self, facing: Facing, walking: bool, walk_frame: usize) -> usize {
        self.validate();
        match self.mapping {
            SpriteSheetMapping::DirectionByColumn {
                facing_columns,
                idle_row,
                walk_start_row,
                walk_frames,
            } => {
                let col = facing_columns.get(facing);
                let row = if walking {
                    walk_start_row + (walk_frame % walk_frames)
                } else {
                    idle_row
                };
                row * self.atlas_columns + col
            }
            SpriteSheetMapping::DirectionByRow {
                facing_rows,
                idle_col,
                walk_start_col,
                walk_frames,
            } => {
                let row = facing_rows.get(facing);
                let col = if walking {
                    walk_start_col + (walk_frame % walk_frames)
                } else {
                    idle_col
                };
                row * self.atlas_columns + col
            }
        }
    }

    pub fn walk_frames(&self) -> usize {
        match self.mapping {
            SpriteSheetMapping::DirectionByColumn { walk_frames, .. } => walk_frames,
            SpriteSheetMapping::DirectionByRow { walk_frames, .. } => walk_frames,
        }
    }
}

#[derive(Component, Debug)]
pub struct SpriteSheetAnimator {
    pub facing: Facing,
    pub walking: bool,
    pub walk_frame: usize,
    pub timer: Timer,
}

impl SpriteSheetAnimator {
    pub fn new(initial_facing: Facing, frame_time_secs: f32) -> Self {
        Self {
            facing: initial_facing,
            walking: false,
            walk_frame: 0,
            timer: Timer::from_seconds(frame_time_secs.max(0.001), TimerMode::Repeating),
        }
    }
}

pub fn make_sprite_with_animator(
    image: Handle<Image>,
    config: SpriteSheetConfig,
    initial_facing: Facing,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (Sprite, SpriteSheetAnimator, SpriteSheetConfig) {
    let layout = config.layout(layouts);
    let mut sprite = Sprite::from_image(image);
    sprite.texture_atlas = Some(TextureAtlas { layout, index: 0 });

    let animator = SpriteSheetAnimator::new(initial_facing, config.frame_time_secs);
    apply_animator_to_sprite(&mut sprite, &config, &animator);
    (sprite, animator, config)
}

pub fn apply_animator_to_sprite(
    sprite: &mut Sprite,
    config: &SpriteSheetConfig,
    animator: &SpriteSheetAnimator,
) {
    if let Some(atlas) = sprite.texture_atlas.as_mut() {
        atlas.index = config.atlas_index(animator.facing, animator.walking, animator.walk_frame);
    }
}

pub fn tick_animator(animator: &mut SpriteSheetAnimator, delta: f32, walk_frames: usize) {
    if !animator.walking {
        animator.walk_frame = 0;
        animator.timer.reset();
        return;
    }

    if animator.timer.tick(std::time::Duration::from_secs_f32(delta)).just_finished() {
        animator.walk_frame = (animator.walk_frame + 1) % walk_frames.max(1);
    }
}

pub fn facing_and_movement_from_input(input: &ButtonInput<KeyCode>) -> (Facing, bool, bool) {
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;

    if input.pressed(KeyCode::KeyW) || input.pressed(KeyCode::ArrowUp) {
        y += 1.0;
    }
    if input.pressed(KeyCode::KeyS) || input.pressed(KeyCode::ArrowDown) {
        y -= 1.0;
    }
    if input.pressed(KeyCode::KeyA) || input.pressed(KeyCode::ArrowLeft) {
        x -= 1.0;
    }
    if input.pressed(KeyCode::KeyD) || input.pressed(KeyCode::ArrowRight) {
        x += 1.0;
    }

    let walking = x != 0.0 || y != 0.0;
    let facing = if y > 0.0 {
        Facing::Up
    } else if y < 0.0 {
        Facing::Down
    } else if x < 0.0 {
        Facing::Left
    } else if x > 0.0 {
        Facing::Right
    } else {
        Facing::Down
    };

    let flip_x = x < 0.0;
    (facing, walking, flip_x)
}
