#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use self::unix::stdin_fd;
#[cfg(unix)]
pub(crate) use self::unix::*;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) use self::windows::*;
