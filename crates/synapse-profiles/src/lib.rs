pub mod error;
pub mod parser;
pub mod resolver;
mod toml_format;
pub mod watcher;

pub use error::{ProfileError, ProfileLoadError};
pub use parser::{
    LoadedProfile, ProfileDefaults, ScreenBounds, bundled_profiles_dir, parse_profile_bytes,
    parse_profile_file, parse_profile_file_with_bounds,
};
pub use resolver::{ForegroundWindow, ProfileMatchResolution, resolve_active_profile};
pub use watcher::{ProfileEventExtensionStatus, ProfileRuntime, ProfileStatus};
