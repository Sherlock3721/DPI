#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub mod bracket;
pub mod extrusion;
pub mod geometry;
pub mod gcode;
pub mod layout;
pub mod metadata;
pub mod path_processing;
pub mod svg_parser;
pub mod types;

pub use bracket::{
    build_bracket_stl, compute_bracket_geometry, generate_bracket_svg, BracketGeometry,
    BracketParams,
};
pub use extrusion::ExtrusionCalculator;
pub use geometry::{
    bbox_of_paths, check_paths_overflow, compute_preview_segments, compute_world_aabb,
    fit_transforms_to_layout,
};
pub use gcode::{
    generate_gcode, generate_prime_preview, is_extrusion_move, is_linear_move, parse_axis_value,
    parse_gcode_paths, parse_move_axes,
};
pub use layout::{get_layout_positions, update_layout, LayoutUpdateResult};
pub use metadata::{build_csv_protocol, deserialize_metadata, serialize_metadata, GCodeMetadata};
pub use path_processing::{parse_dxf, process_substrate_paths};
pub use svg_parser::parse_svg;
pub use types::{
    BedConfig, LayoutPosition, LayoutWithTransforms, MachineConfig, PathSegment, Point2D,
    PreviewDistResult, PreviewSegData, ProcessParams, SliceParams, SlideDimensions, SlideOverride,
    SubstratePaths, Transform,
};
