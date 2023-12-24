use std::{
    fmt::Display,
    io::{self, IsTerminal},
    os::unix::process::CommandExt,
    process::{Child, Command},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use nix::{
    sys::{
        signal::{killpg, sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{self, Pid},
};

use crate::JobId;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Completed,
    Stopped,
    Running,
}

impl Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                JobStatus::Completed => "done",
                JobStatus::Stopped => "stopped",
                JobStatus::Running => "running",
            }
        )
    }
}

pub struct Job {
    pub id: JobId,
    pub command: String,
    pub status: JobStatus,
    // span?
}

struct InternalJob {
    id: JobId,
    command: String,
    pgroup: Pid,
    runnning: Vec<Pid>,
    stopped: Vec<Pid>,
    completed: Vec<Pid>,
}

impl InternalJob {
    fn status(&self) -> JobStatus {
        if !self.runnning.is_empty() {
            JobStatus::Running
        } else if !self.stopped.is_empty() {
            JobStatus::Stopped
        } else {
            JobStatus::Completed
        }
    }

    fn to_job(&self) -> Job {
        Job {
            id: self.id,
            command: self.command.clone(),
            status: self.status(),
        }
    }

    fn mark_process(&mut self, pid: Pid, status: JobStatus) -> bool {
        fn try_move(from: &mut Vec<Pid>, to: &mut Vec<Pid>, pid: Pid) -> bool {
            if let Some(i) = from.iter().position(|&p| p == pid) {
                from.swap_remove(i);
                to.push(pid);
                true
            } else {
                false
            }
        }

        match status {
            JobStatus::Completed => {
                try_move(&mut self.runnning, &mut self.completed, pid)
                    || try_move(&mut self.stopped, &mut self.completed, pid)
            }
            JobStatus::Stopped => try_move(&mut self.runnning, &mut self.stopped, pid),
            JobStatus::Running => try_move(&mut self.stopped, &mut self.runnning, pid),
        }
    }
}

struct JobState {
    foreground: Option<usize>,
    background: Vec<InternalJob>,
}

impl JobState {
    fn foreground_job_mut(&mut self) -> Option<&mut InternalJob> {
        self.foreground.map(|i| &mut self.background[i])
    }

    fn foreground_pgroup(&self) -> Option<Pid> {
        self.foreground.map(|i| self.background[i].pgroup)
    }

    fn mark_process(&mut self, pid: Pid, status: JobStatus) -> Option<&InternalJob> {
        self.background.iter_mut().find_map(|job| {
            if job.mark_process(pid, status) {
                Some(&*job)
            } else {
                None
            }
        })
    }
}

pub struct Jobs {
    next_id: AtomicUsize,
    state: Mutex<JobState>,
}

fn pid(child: &Child) -> Pid {
    Pid::from_raw(child.id() as i32)
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_id(&self) -> usize {
        // We don't care about the order of assigned ids, so we use `Ordering::Relaxed`.
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn new_job(&self, command: String, child: &Child) -> InternalJob {
        let pid = Pid::from_raw(child.id() as i32);
        InternalJob {
            id: self.next_id(),
            command,
            pgroup: pid,
            runnning: vec![pid],
            stopped: Vec::new(),
            completed: Vec::new(),
        }
    }

    pub fn spawn_foreground(&self, mut command: Command, interactive: bool) -> io::Result<Child> {
        let mut state = self.state.lock().expect("unpoisoned");
        let interactive = interactive && io::stdin().is_terminal();
        if interactive {
            prepare_interactive(&mut command, true, state.foreground_pgroup());
        }
        match command.spawn() {
            Ok(child) => {
                if let Some(foreground) = state.foreground_job_mut() {
                    foreground.runnning.push(pid(&child));
                } else {
                    let job = self.new_job(
                        command.get_program().to_owned().into_string().unwrap(),
                        &child,
                    );
                    if interactive {
                        let _ = unistd::setpgid(job.pgroup, job.pgroup);
                        if let Err(e) = unistd::tcsetpgrp(libc::STDIN_FILENO, job.pgroup) {
                            eprintln!("ERROR: failed to set foreground job: {e}");
                        }
                    }
                    state.foreground = Some(state.background.len());
                    state.background.push(job);
                }
                Ok(child)
            }
            Err(e) => {
                if interactive && state.foreground.is_none() {
                    reset_foreground();
                }
                Err(e)
            }
        }
    }

    pub fn spawn_background(&self, mut command: Command, interactive: bool) -> io::Result<Child> {
        if interactive && io::stdin().is_terminal() {
            prepare_interactive(&mut command, false, None);
        }

        let mut state = self.state.lock().expect("unpoisoned");
        let child = command.spawn()?;
        state.background.push(self.new_job(
            command.get_program().to_owned().into_string().unwrap(),
            &child,
        ));
        Ok(child)
    }

    /// Blocks on the foreground process group, waiting until all of its processes
    /// have either stopped or completed. It then restores the terminal, putting nushell back in control.
    pub fn wait_reset_foreground(&self, interactive: bool) {
        let mut state = self.state.lock().expect("unpoisoned");

        let Some(foreground) = state.foreground_pgroup() else {
            return;
        };

        let flags = Some(WaitPidFlag::WUNTRACED);
        while let Ok(status) = waitpid(None, flags) {
            let job = match status {
                WaitStatus::Exited(pid, _code) => state.mark_process(pid, JobStatus::Completed),
                WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                    state.mark_process(pid, JobStatus::Completed)
                }
                WaitStatus::Stopped(pid, _signal) => state.mark_process(pid, JobStatus::Stopped),
                WaitStatus::Continued(_) => unreachable!("WCONTINUED was not provided"),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                    state.mark_process(pid, JobStatus::Stopped)
                }
                WaitStatus::StillAlive => unreachable!("WNOHANG was not provided"),
            };

            debug_assert!(job.is_some());

            if let Some(job) = job {
                if job.pgroup == foreground && job.status() != JobStatus::Running {
                    state.foreground = None;
                    break;
                }
            }
        }

        if interactive && io::stdin().is_terminal() {
            reset_foreground()
        }
    }

    pub fn background_jobs(&self) -> Vec<Job> {
        let mut state = self.state.lock().expect("unpoisoned");

        let flags = Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED | WaitPidFlag::WNOHANG);
        while let Ok(status) = waitpid(None, flags) {
            let job = match status {
                WaitStatus::Exited(pid, _code) => state.mark_process(pid, JobStatus::Completed),
                WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                    state.mark_process(pid, JobStatus::Completed)
                }
                WaitStatus::Stopped(pid, _signal) => state.mark_process(pid, JobStatus::Stopped),
                WaitStatus::Continued(pid) => state.mark_process(pid, JobStatus::Running),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                    state.mark_process(pid, JobStatus::Stopped)
                }
                WaitStatus::StillAlive => break,
            };

            debug_assert!(job.is_some());
        }

        state.background.iter().map(InternalJob::to_job).collect()
    }

    /// Brings a background job to the foreground.
    /// Does nothing if there already is a foreground job or the background job is finished.
    /// Otherwise returns `false` if no job exists with the given [`JobId`].
    pub fn switch_foreground(&self, id: JobId) -> bool {
        let mut state = self.state.lock().expect("unpoisoned");

        if state.foreground.is_some() {
            return true;
        }

        let flags = Some(WaitPidFlag::WNOHANG);
        while let Ok(status) = waitpid(None, flags) {
            let job = match status {
                WaitStatus::Exited(pid, _code) => state.mark_process(pid, JobStatus::Completed),
                WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                    state.mark_process(pid, JobStatus::Completed)
                }
                WaitStatus::Stopped(_, _) => unreachable!("WUNTRACED was not provided"),
                WaitStatus::Continued(_) => unreachable!("WCONTINUED was not provided"),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                    state.mark_process(pid, JobStatus::Stopped)
                }
                WaitStatus::StillAlive => break,
            };

            debug_assert!(job.is_some());
        }

        if let Some(i) = state.background.iter().position(|j| j.id == id) {
            let job = &state.background[i];

            if job.status() == JobStatus::Completed {
                return true;
            }

            if let Err(e) = unistd::tcsetpgrp(libc::STDIN_FILENO, job.pgroup) {
                eprintln!("ERROR: failed to set foreground job: {e}");
                return true;
            }
            if let Err(e) = killpg(job.pgroup, Signal::SIGCONT) {
                eprintln!("ERROR: failed to send SIGCONT: {e}");
                reset_foreground();
                return true;
            }
            state.foreground = Some(i);
            true
        } else {
            false
        }
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            next_id: AtomicUsize::new(1),
            state: Mutex::new(JobState {
                foreground: None,
                background: Vec::new(),
            }),
        }
    }
}

fn prepare_interactive(command: &mut Command, foreground: bool, pgroup: Option<Pid>) {
    unsafe {
        // Safety:
        // POSIX only allows async-signal-safe functions to be called.
        // `sigaction`, `getpid`, `setpgid`, and `tcsetpgrp` are async-signal-safe according to:
        // https://manpages.ubuntu.com/manpages/bionic/man7/signal-safety.7.html
        command.pre_exec(move || {
            // When this callback is run, std::process has already:
            // - reset SIGPIPE to SIG_DFL

            let pid = unistd::getpid();
            let pgroup = pgroup.unwrap_or(pid);

            // According to glibc's job control manual:
            // https://www.gnu.org/software/libc/manual/html_node/Launching-Jobs.html
            // This has to be done *both* in the parent and here in the child due to race conditions.
            let _ = unistd::setpgid(pid, pgroup);
            if foreground {
                let _ = unistd::tcsetpgrp(libc::STDIN_FILENO, pgroup);
            }

            // Reset signal handlers for child, sync with `terminal.rs`
            let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
            // SIGINT has special handling
            sigaction(Signal::SIGQUIT, &default).expect("signal default");
            sigaction(Signal::SIGTSTP, &default).expect("signal default");
            sigaction(Signal::SIGTTIN, &default).expect("signal default");
            sigaction(Signal::SIGTTOU, &default).expect("signal default");

            // TODO: determine if this is necessary or not, since this breaks `rm` on macOS
            // sigaction(Signal::SIGCHLD, &ignore).expect("signal default");

            sigaction(Signal::SIGTERM, &default).expect("signal default");

            Ok(())
        });
    }
}

/// Makes nushell the owner of the terminal again (the foreground process group)
fn reset_foreground() {
    if let Err(e) = unistd::tcsetpgrp(libc::STDIN_FILENO, unistd::getpgrp()) {
        eprintln!("ERROR: failed to set foreground job: {e}");
    }
}
