# sprite_sheet.rs Usage

This module provides a reusable, config-driven API for sprite sheet animation.

## Location
- Module: `src/sprite_sheet.rs`
- Export: `robsim::sprite_sheet`

## Core Types
- `Facing`: `Down | Up | Left | Right`
- `SpriteSheetConfig`: atlas + animation mapping config
- `SpriteSheetAnimator`: runtime animation state component
- `SpriteSheetMapping`:
  - `DirectionByRow`
  - `DirectionByColumn`

## Quick Start (Common RPG Sheet)
If your sheet is:
- rows = direction (`down`, `up`, `side`)
- columns = frames (`idle`, `walk1`, `walk2`)

use:

```rust
use robsim::sprite_sheet::{
    Facing, SpriteSheetConfig, make_sprite_with_animator,
};

let config = SpriteSheetConfig::simple_4dir_rows(
    16, // tile px
    3,  // atlas columns
    3,  // atlas rows
    0,  // down row
    1,  // up row
    2,  // side row (left+right)
    0.15, // frame time
);

let image = assets.load("robber.png");
let (sprite, animator, config) = make_sprite_with_animator(
    image,
    config,
    Facing::Down,
    &mut texture_atlas_layouts,
);

commands.spawn((sprite, animator, config));
```

## Fully Custom Config

### Direction by row
```rust
use robsim::sprite_sheet::{FacingRows, SpriteSheetConfig};

let config = SpriteSheetConfig::from_grid_direction_by_row(
    UVec2::splat(16),
    3, // atlas columns
    3, // atlas rows
    FacingRows::new(0, 1, 2, 2), // down, up, left, right rows
    0, // idle column
    1, // walk start column
    2, // walk frame count
    0.12,
);
```

### Direction by column
```rust
use robsim::sprite_sheet::{FacingColumns, SpriteSheetConfig};

let config = SpriteSheetConfig::from_grid_direction_by_column(
    UVec2::splat(16),
    4, // atlas columns
    3, // atlas rows
    FacingColumns::new(0, 1, 2, 3), // down, up, left, right columns
    0, // idle row
    1, // walk start row
    2, // walk frame count
    0.10,
);
```

## Updating Animation Each Frame
In your movement/update system:

```rust
use robsim::sprite_sheet::{
    facing_and_movement_from_input,
    tick_animator,
    apply_animator_to_sprite,
};
x
let (facing, walking, flip_x) = facing_and_movement_from_input(&keyboard_input);
animator.facing = facing;
animator.walking = walking;
sprite.flip_x = flip_x;

tick_animator(&mut animator, time.delta_secs(), config.walk_frames());
apply_animator_to_sprite(&mut sprite, &config, &animator);
```

## API Summary
- `SpriteSheetConfig::layout(...)`
  - Builds and registers a `TextureAtlasLayout`.
- `SpriteSheetConfig::atlas_index(...)`
  - Computes atlas frame index for `(facing, walking, walk_frame)`.
- `SpriteSheetConfig::walk_frames()`
  - Returns configured walk frame count.
- `make_sprite_with_animator(...)`
  - Convenience helper to create a sprite with atlas + animator + config.

## Notes
- `validate()` is called by layout/index methods and asserts invalid config early.
- If left/right share art, map both to the same row/column and use `sprite.flip_x`.
