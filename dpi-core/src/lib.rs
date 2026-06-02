pub mod extrusion;
pub mod geometry;
pub mod gcode;
pub mod layout;
pub mod metadata;
pub mod path_processing;
pub mod svg_parser;
pub mod types;

pub use extrusion::ExtrusionCalculator;
pub use geometry::{bbox_of_paths, compute_world_aabb, fit_transforms_to_layout};
pub use gcode::{generate_gcode, generate_prime_preview, parse_gcode_paths};
pub use layout::get_layout_positions;
pub use metadata::{build_csv_protocol, deserialize_metadata, serialize_metadata, GCodeMetadata};
pub use path_processing::{parse_dxf, process_substrate_paths};
pub use svg_parser::parse_svg;
pub use types::{
    BedConfig, LayoutPosition, LayoutWithTransforms, MachineConfig, PathSegment, Point2D,
    ProcessParams, SliceParams, SlideDimensions, SlideOverride, SubstratePaths, Transform,
};
