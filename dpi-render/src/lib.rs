//! Renderer 3D náhledu tiskové plochy (wgpu). Stejný kód běží nativně
//! (offscreen v src-tauri) i jako wasm nad WebGL2 přímo v plátně.
pub mod cursor;
mod meshes;
pub mod protocol;
mod renderer;
#[doc(hidden)]
pub mod testkit;

pub use renderer::{device_limits, pick_formats, AdapterSummary, Renderer, MAX_OBJECTS};

#[cfg(feature = "native")]
pub mod offscreen;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
mod wasm;
