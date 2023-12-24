mod job;
#[cfg(any(target_os = "android", target_os = "linux"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
pub mod os_info;
#[cfg(target_os = "windows")]
mod windows;

pub use job::*;
#[cfg(any(target_os = "android", target_os = "linux"))]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
pub use windows::*;
