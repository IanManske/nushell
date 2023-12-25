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
    /// The number of stopped processes in this job.
    stopped: usize,
    /// All the pids of the processes in this job.
    /// This is partitioned according to `stopped`:
    /// - all pids before index `stopped` are considered to be stopped
    /// - all pids at and after index `stopped` are considered to be running
    /// - processes that have completed are removed
    processes: Vec<Pid>,
}

impl InternalJob {
    fn status(&self) -> JobStatus {
        if self.processes.is_empty() {
            JobStatus::Completed
        } else if self.stopped == self.processes.len() {
            JobStatus::Stopped
        } else {
            JobStatus::Running
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
        if let Some(i) = self.processes.iter().position(|&p| p == pid) {
            match status {
                JobStatus::Completed => {
                    if i < self.stopped {
                        self.processes.swap(i, self.stopped - 1);
                        self.processes.swap_remove(self.stopped - 1);
                    } else {
                        self.processes.swap_remove(i);
                    }
                }
                JobStatus::Stopped => {
                    debug_assert!(i >= self.stopped);
                    self.processes.swap(i, self.stopped);
                    self.stopped += 1;
                }
                JobStatus::Running => {
                    debug_assert!(i < self.stopped);
                    self.processes.swap(i, self.stopped - 1);
                    self.stopped -= 1;
                }
            }
            true
        } else {
            false
        }
    }
}

struct JobState {
    foreground: Option<usize>,
    jobs: Vec<InternalJob>,
}

impl JobState {
    fn mark_process(&mut self, pid: Pid, status: JobStatus) -> Option<&InternalJob> {
        self.jobs.iter_mut().find_map(|job| {
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
            stopped: 0,
            processes: vec![pid],
        }
    }

    pub fn spawn_foreground(&self, mut command: Command, interactive: bool) -> io::Result<Child> {
        let mut state = self.state.lock().expect("unpoisoned");
        let interactive = interactive && io::stdin().is_terminal();
        let foreground = state.foreground.map(|i| &mut state.jobs[i]);
        if interactive {
            prepare_interactive(&mut command, true, foreground.as_deref().map(|j| j.pgroup));
        }
        match command.spawn() {
            Ok(child) => {
                if let Some(foreground) = foreground {
                    foreground.processes.push(pid(&child));
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
                    state.foreground = Some(state.jobs.len());
                    state.jobs.push(job);
                }
                Ok(child)
            }
            Err(e) => {
                if interactive && foreground.is_none() {
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
        state.jobs.push(self.new_job(
            command.get_program().to_owned().into_string().unwrap(),
            &child,
        ));
        Ok(child)
    }

    /// Blocks on the foreground process group, waiting until all of its processes
    /// have either stopped or completed. It then restores the terminal, putting nushell back in control.
    pub fn wait_reset_foreground(&self, interactive: bool) {
        let mut state = self.state.lock().expect("unpoisoned");

        let Some(i) = state.foreground else {
            return;
        };

        let foreground = state.jobs[i].pgroup;

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
                let status = job.status();
                if job.pgroup == foreground && status != JobStatus::Running {
                    if status == JobStatus::Completed {
                        state.jobs.swap_remove(i);
                    }
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

        state.jobs.iter().map(InternalJob::to_job).collect()
    }

    /// Brings a background job to the foreground.
    /// Does nothing if there already is a foreground job.
    /// If the background job is completed, removes it from the list of jobs.
    /// Returns `false` if no job exists with the given [`JobId`].
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

        if let Some(i) = state.jobs.iter().position(|j| j.id == id) {
            let job = &state.jobs[i];

            if job.status() == JobStatus::Completed {
                state.jobs.swap_remove(i);
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
                jobs: Vec::new(),
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
