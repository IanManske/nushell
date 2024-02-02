mod casing;
pub mod ctrl_c;
mod deansi;
pub mod emoji;
pub mod locale;
pub mod utils;

pub use locale::get_system_locale;
pub use utils::{
    enable_vt_processing, get_default_config, get_default_env, get_ls_colors,
    stderr_write_all_and_flush, stdout_write_all_and_flush,
};

pub use casing::IgnoreCaseExt;
pub use deansi::{
    strip_ansi_likely, strip_ansi_string_likely, strip_ansi_string_unlikely, strip_ansi_unlikely,
};
pub use emoji::contains_emoji;
