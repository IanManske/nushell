mod foreground;
#[cfg(any(target_os = "android", target_os = "linux"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
pub mod os_info;
mod sys;
#[cfg(target_os = "windows")]
mod windows;

pub use self::foreground::*;
#[cfg(any(target_os = "android", target_os = "linux"))]
pub use self::linux::*;
#[cfg(target_os = "macos")]
pub use self::macos::*;
#[cfg(unix)]
pub use self::sys::stdin_fd;
#[cfg(target_os = "windows")]
pub use self::windows::*;
