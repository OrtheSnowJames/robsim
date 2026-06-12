use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;

pub struct NineSlicingPlugin;

impl Plugin for NineSlicingPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<NineSliceBorder>()
            .add_systems(Update, apply_nine_slice_border);
    }
}

#[derive(Component, Debug, Clone, Reflect)]
pub struct NineSliceBorder {
    pub border_insets: Vec4,
}

fn apply_nine_slice_border(
    mut query: Query<
        (&NineSliceBorder, &mut ImageNode),
        Or<(Added<NineSliceBorder>, Changed<NineSliceBorder>)>,
    >,
) {
    for (nine_slice, mut image_node) in &mut query {
        let insets = nine_slice.border_insets;
        let slicer = TextureSlicer {
            border: BorderRect::from([insets.x, insets.y, insets.z, insets.w]),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: 1.0,
        };
        image_node.image_mode = NodeImageMode::Sliced(slicer);
    }
}
