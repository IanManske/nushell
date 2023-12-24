use std::{
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
        signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{self, Pid},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Completed,
    Stopped,
    Running,
}

pub struct Job {
    id: usize,
    pgroup: Pid,
    runnning: Vec<Pid>,
    stopped: Vec<Pid>,
    completed: Vec<Pid>,
}

impl Job {
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn status(&self) -> JobStatus {
        if !self.runnning.is_empty() {
            JobStatus::Running
        } else if !self.stopped.is_empty() {
            JobStatus::Stopped
        } else {
            JobStatus::Completed
        }
    }

    fn mark_process(&mut self, pid: Pid, status: JobStatus) {
        fn try_move(from: &mut Vec<Pid>, to: &mut Vec<Pid>, pid: Pid) -> bool {
            if let Some(i) = from.iter().position(|&p| p == pid) {
                from.swap_remove(i);
                to.push(pid);
                true
            } else {
                false
            }
        }

        let moved = match status {
            JobStatus::Completed => {
                try_move(&mut self.runnning, &mut self.completed, pid)
                    || try_move(&mut self.stopped, &mut self.completed, pid)
            }
            JobStatus::Stopped => try_move(&mut self.runnning, &mut self.stopped, pid),
            JobStatus::Running => try_move(&mut self.stopped, &mut self.runnning, pid),
        };

        debug_assert!(moved)
    }
}

pub struct Jobs {
    next_id: AtomicUsize,
    foreground_job: Mutex<Option<Job>>,
    background_jobs: Mutex<Vec<Job>>,
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

    fn new_job(&self, child: &Child) -> Job {
        let pid = Pid::from_raw(child.id() as i32);
        Job {
            id: self.next_id(),
            pgroup: pid,
            runnning: vec![pid],
            stopped: Vec::new(),
            completed: Vec::new(),
        }
    }

    pub fn spawn_foreground(&self, mut command: Command, interactive: bool) -> io::Result<Child> {
        let mut foreground = self.foreground_job.lock().expect("unpoisoned");
        let interactive = interactive && io::stdin().is_terminal();
        if interactive {
            prepare_interactive(
                &mut command,
                foreground
                    .as_ref()
                    .map(|j| j.pgroup)
                    .or(Some(Pid::from_raw(0))),
            );
        }
        let child = command.spawn()?;
        if let Some(foreground) = foreground.as_mut() {
            foreground.runnning.push(pid(&child));
        } else {
            let job = self.new_job(&child);
            if interactive {
                set_foreground_pid(job.pgroup, job.pgroup);
            }
            *foreground = Some(job);
        }
        Ok(child)
    }

    pub fn spawn_background(&self, mut command: Command, interactive: bool) -> io::Result<Child> {
        if interactive && io::stdin().is_terminal() {
            prepare_interactive(&mut command, None);
        }

        let mut background = self.background_jobs.lock().expect("unpoisoned");
        let child = command.spawn()?;
        background.push(self.new_job(&child));
        Ok(child)
    }

    /// Blocks on the foreground process group, waiting until all process have either stopped or completed.
    /// It then restores the terminal, putting nushell back in control.
    pub fn block_reset_foreground(&self, interactive: bool) {
        let pgroup = {
            self.foreground_job
                .lock()
                .expect("unpoisoned")
                .as_ref()
                .map(|j| j.pgroup)
        };

        if let Some(pgroup) = pgroup {
            let flags = Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED);
            while let Ok(status) = waitpid(Pid::from_raw(-pgroup.as_raw()), flags) {
                let mut foreground = self.foreground_job.lock().expect("unpoisoned");
                let job = foreground.as_mut().expect("foreground exists");

                match status {
                    WaitStatus::Exited(pid, _code) => job.mark_process(pid, JobStatus::Completed),
                    WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                        job.mark_process(pid, JobStatus::Completed)
                    }
                    WaitStatus::Stopped(pid, _signal) => job.mark_process(pid, JobStatus::Stopped),
                    WaitStatus::Continued(pid) => job.mark_process(pid, JobStatus::Running),
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    WaitStatus::PtraceEvent(_, _, _) | WaitStatus::PtraceSyscall(_) => (),
                    WaitStatus::StillAlive => unreachable!("WNOHANG was not provided"),
                }

                match job.status() {
                    JobStatus::Completed => {
                        *foreground = None;
                        break;
                    }
                    JobStatus::Stopped => {
                        self.background_jobs
                            .lock()
                            .expect("unpoisoned")
                            .push(foreground.take().expect("foreground exists"));

                        break;
                    }
                    JobStatus::Running => (),
                }
            }

            if interactive && io::stdin().is_terminal() {
                // Make nushell the owner of the terminal again (the foreground process group)
                if let Err(e) = unistd::tcsetpgrp(libc::STDIN_FILENO, unistd::getpgrp()) {
                    eprintln!("ERROR: tcsetpgrp failed: {e:?}");
                }
            }
        }
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            next_id: AtomicUsize::new(1),
            foreground_job: Mutex::new(None),
            background_jobs: Mutex::new(Vec::new()),
        }
    }
}

fn prepare_interactive(command: &mut Command, foreground_pgroup: Option<Pid>) {
    unsafe {
        // Safety:
        // POSIX only allows async-signal-safe functions to be called.
        // `sigaction` and `getpid` are async-signal-safe according to:
        // https://manpages.ubuntu.com/manpages/bionic/man7/signal-safety.7.html
        // Also, `set_foreground_pid` is async-signal-safe.
        command.pre_exec(move || {
            // When this callback is run, std::process has already:
            // - reset SIGPIPE to SIG_DFL

            // According to glibc's job control manual:
            // https://www.gnu.org/software/libc/manual/html_node/Launching-Jobs.html
            // This has to be done *both* in the parent and here in the child due to race conditions.
            if let Some(pgroup) = foreground_pgroup {
                set_foreground_pid(unistd::getpid(), pgroup);
            }

            // Reset signal handlers for child, sync with `terminal.rs`
            let default = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
            // SIGINT has special handling
            sigaction(Signal::SIGQUIT, &default).expect("signal default");
            // We don't support background jobs, so keep SIGTSTP blocked?
            // sigaction(Signal::SIGTSTP, &default).expect("signal default");
            sigaction(Signal::SIGTTIN, &default).expect("signal default");
            sigaction(Signal::SIGTTOU, &default).expect("signal default");

            // TODO: determine if this is necessary or not, since this breaks `rm` on macOS
            // sigaction(Signal::SIGCHLD, &ignore).expect("signal default");

            sigaction(Signal::SIGTERM, &default).expect("signal default");

            Ok(())
        });
    }
}

fn set_foreground_pid(pid: Pid, pgroup: Pid) {
    // Safety: needs to be async-signal-safe.
    // `setpgid` and `tcsetpgrp` are async-signal-safe.
    let pgroup = if pgroup.as_raw() == 0 { pid } else { pgroup };
    let _ = unistd::setpgid(pid, pgroup);
    let _ = unistd::tcsetpgrp(libc::STDIN_FILENO, pgroup);
}
