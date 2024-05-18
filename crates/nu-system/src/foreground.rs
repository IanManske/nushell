use crate::sys;
use std::{
    io,
    process::{Child, Command, ExitStatus},
};

#[derive(Debug, Clone)]
pub struct ForegroundState {
    inner: sys::ForegroundState,
}

impl ForegroundState {
    pub fn new() -> Self {
        Self {
            inner: sys::ForegroundState::new(),
        }
    }

    /// Resets the foreground, giving this process (e.g., the shell) back control of the terminal.
    ///
    /// # Safety
    /// There should be no child processes that have control of the terminal.
    /// Otherwise this will take control of the terminal out from under them.
    ///
    /// # OS-specific behavior
    /// ## Unix
    ///
    /// Calls `tcsetpgrp` to set the shell's pgid as the terminal foreground process group.
    ///
    /// ## Windows
    ///
    /// This is currently a no-op on Windows.
    pub unsafe fn force_reset(&self) {
        unsafe { self.inner.force_reset() }
    }
}

impl Default for ForegroundState {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple wrapper for [`std::process::Child`]
///
/// It can only be created by [`ForegroundChild::spawn`].
///
/// # Spawn behavior
/// ## Unix
///
/// For interactive shells, the spawned child process will get its own process group id,
/// and it will be put in the foreground (by making stdin belong to the child's process group).
/// On drop, the calling process's group will become the foreground process group once again.
///
/// For non-interactive mode, processes are spawned normally without any foreground process handling.
///
/// ## Windows
///
/// It does nothing special on Windows, so `spawn` is the same as [`std::process::Command::spawn`].
pub struct ForegroundChild {
    inner: sys::ForegroundChild,
}

impl ForegroundChild {
    pub fn spawn(command: Command, interactive: bool, state: &ForegroundState) -> io::Result<Self> {
        sys::ForegroundChild::spawn(command, interactive, &state.inner).map(|inner| Self { inner })
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.as_mut().wait()
    }
}

impl AsMut<Child> for ForegroundChild {
    fn as_mut(&mut self) -> &mut Child {
        self.inner.as_mut()
    }
}

/// Keeps a specific already existing process in the foreground as long as the [`ForegroundGuard`].
/// If the process needs to be spawned in the foreground, use [`ForegroundChild`] instead. This is
/// used to temporarily bring plugin processes into the foreground.
///
/// # OS-specific behavior
/// ## Unix
///
/// If there is already a foreground external process running, spawned with [`ForegroundChild`],
/// this expects the process ID to remain in the process group created by the [`ForegroundChild`]
/// for the lifetime of the guard, and keeps the terminal controlling process group set to that. If
/// there is no foreground external process running, this sets the foreground process group to the
/// plugin's process ID. The process group that is expected can be retrieved with [`.pgrp()`] if
/// different from the plugin process ID.
///
/// ## Windows
///
/// It does nothing special on Windows.
#[derive(Debug)]
pub struct ForegroundGuard {
    inner: sys::ForegroundGuard,
}

impl ForegroundGuard {
    /// Move the given process to the foreground.
    pub fn new(pid: u32, state: &ForegroundState) -> io::Result<ForegroundGuard> {
        sys::ForegroundGuard::new(pid, &state.inner).map(|inner| Self { inner })
    }

    /// If the child process is expected to join a different process group to be in the foreground,
    /// this returns `Some(pgrp)`. This only ever returns `Some` on Unix.
    pub fn pgroup(&self) -> Option<u32> {
        self.inner.pgroup()
    }
}
