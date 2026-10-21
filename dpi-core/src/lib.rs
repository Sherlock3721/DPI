#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub mod bracket;
pub mod extrusion;
pub mod geometry;
pub mod gcode;
pub mod layout;
pub mod metadata;
pub mod path_processing;
pub mod regions;
pub mod schedule;
pub mod svg_parser;
pub mod types;

pub use bracket::{
    build_bracket_stl, compute_bracket_geometry, generate_bracket_svg, BracketGeometry,
    BracketParams,
};
pub use extrusion::ExtrusionCalculator;
pub use geometry::{
    bbox_of_paths, check_paths_overflow, clip_paths_to_slide, compute_preview_segments, compute_world_aabb,
    fit_transforms_to_layout,
};
pub use gcode::{
    calibration_glass_z, compute_z_shift, generate_gcode, generate_prime_preview, glass_z_theoretical, print_heights,
    is_extrusion_move, is_linear_move, parse_axis_value, parse_gcode_paths, parse_move_axes,
    split_gcode_by_pauses, DepositHeights, GcodePauseSegment, SlideHeights, DEFAULT_NOZZLE_CHANGE_GCODE, NOZZLE_ALIGN_MARKER,
};
pub use layout::{get_layout_positions, update_layout, LayoutUpdateResult};
pub use metadata::{build_csv_protocol, deserialize_metadata, serialize_metadata, GCodeMetadata};
pub use path_processing::{parse_dxf, process_substrate_paths, split_into_objects};
pub use svg_parser::parse_svg;
pub use types::{
    Anchor, BedConfig, LayoutPosition, LayoutWithTransforms, MachineConfig, MotionConfig, ObjectOverride,
    ObjectTransform, PathObject,
    PathSegment, Placement, PlacementState, Point2D,
    PreviewDistResult, PreviewSegData, ProcessParams, SceneObject, SliceParams, SlideDimensions, SlideOverride,
    SubstratePaths, Transform,
};
