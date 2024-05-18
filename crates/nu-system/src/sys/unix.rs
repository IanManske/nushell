use nix::{
    sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
    unistd::{self, Pid},
};
use std::{
    io::{self, IsTerminal},
    os::{
        fd::{AsFd, BorrowedFd},
        unix::prelude::CommandExt,
    },
    process::{Child, Command},
    sync::{Arc, Mutex, Weak},
};

#[derive(Debug)]
struct ForegroundPgroup(Pid);

impl Drop for ForegroundPgroup {
    fn drop(&mut self) {
        reset_foreground()
    }
}

#[derive(Debug, Clone)]
pub struct ForegroundState {
    pgroup: Arc<Mutex<Weak<ForegroundPgroup>>>,
}

impl ForegroundState {
    pub fn new() -> Self {
        Self {
            pgroup: Arc::new(Mutex::new(Weak::new())),
        }
    }

    pub unsafe fn force_reset(&self) {
        reset_foreground()
    }
}

pub struct ForegroundChild {
    child: Child,
    _pgroup: Option<Arc<ForegroundPgroup>>,
}

impl ForegroundChild {
    pub fn spawn(
        mut command: Command,
        interactive: bool,
        state: &ForegroundState,
    ) -> io::Result<Self> {
        if interactive && io::stdin().is_terminal() {
            // FIXME TOCTOU: child processes can terminate at any point
            // meaning that the strong count of the `Arc`/`Weak` in `state.pgroup`
            // does not reflect the number of processes in the foreground.
            // I.e., we can take the lock, see that `pgroup.is_some()`,
            // but then immediately have the only other process in `pgroup` terminate
            // before we launch this child. This could cause `setpgid` and `tcsetpgrp`
            // in the `pre_exec` below to fail with EPERM?
            let mut pgroup_guard = state.pgroup.lock().expect("unpoisoned lock");
            let pgroup = pgroup_guard.upgrade();
            prepare_command(&mut command, pgroup.as_ref().map(|p| p.0));
            match command.spawn() {
                Ok(child) => {
                    let pgroup = match pgroup {
                        Some(pgroup) => pgroup,
                        None => {
                            let pid = Pid::from_raw(child.id() as i32);
                            let pgroup = Arc::new(ForegroundPgroup(pid));
                            *pgroup_guard = Arc::downgrade(&pgroup);
                            pgroup
                        }
                    };
                    // See the note below in `prepare_command` as to why
                    // this `tcsetpgrp` is necessary for now.
                    let _ = unistd::tcsetpgrp(unsafe { stdin_fd() }, pgroup.0);
                    let _pgroup = Some(pgroup);
                    Ok(Self { child, _pgroup })
                }
                Err(err) => {
                    // The `spawn` could have failed due to an error being communicated back
                    // to this parent process from the spawned child. In that case, the child's
                    // `pre_exec` closure could have run and grabbed control of the terminal.
                    // If the shell was originally in control of terminal, then we need to
                    // give control of the terminal back to the shell.
                    if pgroup.is_none() {
                        reset_foreground();
                    }
                    Err(err)
                }
            }
        } else {
            command.spawn().map(|child| Self {
                child,
                _pgroup: None,
            })
        }
    }
}

impl AsMut<Child> for ForegroundChild {
    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[derive(Debug)]
pub struct ForegroundGuard {
    pgroup: Arc<ForegroundPgroup>,
    leader: bool,
}

impl ForegroundGuard {
    pub fn new(pid: u32, state: &ForegroundState) -> io::Result<ForegroundGuard> {
        let mut pgroup_lock = state.pgroup.lock().expect("unpoisoned lock");
        let foreground = match pgroup_lock.upgrade() {
            Some(pgroup) => Self {
                pgroup,
                leader: false,
            },
            None => {
                let pid = Pid::from_raw(pid as i32);
                unistd::tcsetpgrp(unsafe { stdin_fd() }, pid)?;
                let pgroup = Arc::new(ForegroundPgroup(pid));
                *pgroup_lock = Arc::downgrade(&pgroup);
                Self {
                    pgroup,
                    leader: true,
                }
            }
        };
        Ok(foreground)
    }

    pub fn pgroup(&self) -> Option<u32> {
        (!self.leader).then_some(self.pgroup.0.as_raw() as u32)
    }
}

/// Alternative to having to call `std::io::stdin()` just to get the file descriptor of stdin
///
/// # Safety
/// I/O safety of reading from `STDIN_FILENO` unclear.
///
/// Currently only intended to access `tcsetpgrp` and `tcgetpgrp` with the I/O safe `nix`
/// interface.
pub unsafe fn stdin_fd() -> impl AsFd {
    unsafe { BorrowedFd::borrow_raw(nix::libc::STDIN_FILENO) }
}

fn prepare_command(command: &mut Command, pgroup: Option<Pid>) {
    unsafe {
        // Safety:
        // POSIX only allows async-signal-safe functions to be called after fork.
        // The functions used below include:
        // - `Pid::this` which calls `getpid`
        // - `setpgid`
        // - `tcsetpgrp`
        // - `sigaction`
        // All of these are async-signal-safe according to:
        // https://manpages.ubuntu.com/manpages/bionic/man7/signal-safety.7.html
        command.pre_exec(move || {
            // When this callback is run, `std::process` has already:
            // - reset SIGPIPE to SIG_DFL (since the Rust runtime sets SIGPIPE to SIG_IGN on startup)

            // According to glibc's job control manual:
            // https://www.gnu.org/software/libc/manual/html_node/Launching-Jobs.html
            // The `setpgid` and `tcsetpgrp` have to be done in both the parent and the child
            // due to race conditions. However, `Command::spawn` uses a self-pipe or socket
            // to communicate from the spawned child back to the parent. That is, `Command::spawn`
            // will block, waiting for the child to send a ok message or an error code.
            // Once the message is received in the parent, we are guaranteed that either
            // this closure has finished running or that an error occurred.
            // This avoids the aforementioned race condition.
            let pid = Pid::this();
            let pgroup = pgroup.unwrap_or(pid);
            let _ = unistd::setpgid(pid, pgroup);
            let _ = unistd::tcsetpgrp(stdin_fd(), pgroup);
            // FIXME: stdin may be a pipe and not the terminal, in which case `tcsetpgrp` will fail.
            // E.g., `(1 + 1) | ^external-cmd`
            // So, for now we have the parent shell also run `tcsetpgrp` just in case.
            // However, this means that it is possible for the child process to not be in control
            // of the terminal for a short amount of time.

            // Reset signal handlers for child, sync with `terminal.rs`
            let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
            let _ = sigaction(Signal::SIGQUIT, &default);
            // We don't support background jobs, so keep some signals blocked for now
            // let _ = sigaction(Signal::SIGTSTP, &default);
            // let _ = sigaction(Signal::SIGTTIN, &default);
            // let _ = sigaction(Signal::SIGTTOU, &default);
            // SIGINT AND SIGTERM have handlers which are set to back to SIG_DFL on execve

            Ok(())
        });
    }
}

/// Reset the foreground process group to the shell
fn reset_foreground() {
    if let Err(e) = unistd::tcsetpgrp(unsafe { stdin_fd() }, unistd::getpgrp()) {
        eprintln!("ERROR: reset foreground id failed, tcsetpgrp result: {e:?}");
    }
}
