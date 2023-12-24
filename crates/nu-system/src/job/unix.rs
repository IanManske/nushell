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

        debug_assert!(moved, "failed to find process with id {pid}")
    }
}

pub struct Jobs {
    next_id: AtomicUsize,
    foreground_job: Mutex<Option<InternalJob>>,
    background_jobs: Mutex<Vec<InternalJob>>,
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
        let mut foreground = self.foreground_job.lock().expect("unpoisoned");
        let interactive = interactive && io::stdin().is_terminal();
        if interactive {
            prepare_interactive(&mut command, true, foreground.as_ref().map(|j| j.pgroup));
        }
        match command.spawn() {
            Ok(child) => {
                if let Some(foreground) = foreground.as_mut() {
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
                    *foreground = Some(job);
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

        let mut background = self.background_jobs.lock().expect("unpoisoned");
        let child = command.spawn()?;
        background.push(self.new_job(
            command.get_program().to_owned().into_string().unwrap(),
            &child,
        ));
        Ok(child)
    }

    /// Blocks on the foreground process group, waiting until all of its processes
    /// have either stopped or completed. It then restores the terminal, putting nushell back in control.
    pub fn wait_reset_foreground(&self, interactive: bool) {
        if !interactive {
            return;
        }

        {
            let mut foreground = self.foreground_job.lock().expect("unpoisoned");
            if let Some(job) = foreground.as_mut() {
                let flags = Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED);
                while let Ok(status) = waitpid(Pid::from_raw(-job.pgroup.as_raw()), flags) {
                    match status {
                        WaitStatus::Exited(pid, _code) => {
                            job.mark_process(pid, JobStatus::Completed)
                        }
                        WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                            job.mark_process(pid, JobStatus::Completed)
                        }
                        WaitStatus::Stopped(pid, _signal) => {
                            job.mark_process(pid, JobStatus::Stopped)
                        }
                        WaitStatus::Continued(pid) => job.mark_process(pid, JobStatus::Running),
                        #[cfg(any(target_os = "linux", target_os = "android"))]
                        WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                            job.mark_process(pid, JobStatus::Stopped)
                        }
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

                if io::stdin().is_terminal() {
                    reset_foreground()
                }
            }
        }

        {
            let mut background = self.background_jobs.lock().expect("unpoisoned");

            let mut try_mark_job = |pid, status| match unistd::getpgid(Some(pid)) {
                Ok(pgroup) => {
                    if let Some(job) = background.iter_mut().find(|j| j.pgroup == pgroup) {
                        job.mark_process(pid, status)
                    } else {
                        debug_assert!(false, "unknown pgroup {pgroup}");
                    }
                }
                Err(e) => {
                    eprint!("ERROR: failed to get process group: {e}")
                }
            };

            let flags = Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED);
            while let Ok(status) = waitpid(None, flags) {
                match status {
                    WaitStatus::Exited(pid, _code) => try_mark_job(pid, JobStatus::Completed),
                    WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                        try_mark_job(pid, JobStatus::Completed)
                    }
                    WaitStatus::Stopped(pid, _signal) => try_mark_job(pid, JobStatus::Stopped),
                    WaitStatus::Continued(pid) => try_mark_job(pid, JobStatus::Running),
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                        try_mark_job(pid, JobStatus::Stopped)
                    }
                    WaitStatus::StillAlive => unreachable!("WNOHANG was not provided"),
                }
            }
        }
    }

    pub fn background_jobs(&self) -> Vec<Job> {
        let mut foreground = self.foreground_job.lock().expect("unpoisoned");
        let mut background = self.background_jobs.lock().expect("unpoisoned");

        let mut try_mark_job = |pid, status| match unistd::getpgid(Some(pid)) {
            Ok(pgroup) => match foreground.as_mut() {
                Some(foreground) if pgroup == foreground.pgroup => {
                    foreground.mark_process(pid, status)
                }
                _ => {
                    if let Some(job) = background.iter_mut().find(|j| j.pgroup == pgroup) {
                        job.mark_process(pid, status)
                    } else {
                        debug_assert!(false, "unknown pgroup {pgroup}");
                    }
                }
            },
            Err(e) => {
                eprint!("ERROR: failed to get process group: {e}")
            }
        };

        let flags = Some(WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED);
        while let Ok(status) = waitpid(None, flags) {
            match status {
                WaitStatus::Exited(pid, _code) => try_mark_job(pid, JobStatus::Completed),
                WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                    try_mark_job(pid, JobStatus::Completed)
                }
                WaitStatus::Stopped(pid, _signal) => try_mark_job(pid, JobStatus::Stopped),
                WaitStatus::Continued(pid) => try_mark_job(pid, JobStatus::Running),
                #[cfg(any(target_os = "linux", target_os = "android"))]
                WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                    try_mark_job(pid, JobStatus::Stopped)
                }
                WaitStatus::StillAlive => unreachable!("WNOHANG was not provided"),
            }
        }

        background.iter().map(InternalJob::to_job).collect()
    }

    /// Brings a background job to the foreground.
    /// Does nothing if there already is a foreground job or the background job is finished.
    /// Otherwise returns `false` if no job exists with the given [`JobId`].
    pub fn switch_foreground(&self, id: JobId) -> bool {
        let mut foreground = self.foreground_job.lock().expect("unpoisoned");

        if foreground.is_some() {
            return true;
        }

        let mut background = self.background_jobs.lock().expect("unpoisoned");

        if let Some(i) = background.iter().position(|j| j.id == id) {
            let job = &mut background[i];

            let flags = Some(WaitPidFlag::WNOHANG);
            while let Ok(status) = waitpid(Pid::from_raw(-job.pgroup.as_raw()), flags) {
                match status {
                    WaitStatus::Exited(pid, _code) => job.mark_process(pid, JobStatus::Completed),
                    WaitStatus::Signaled(pid, _signal, _core_dumped) => {
                        job.mark_process(pid, JobStatus::Completed)
                    }
                    WaitStatus::Stopped(_, _) => unreachable!("WUNTRACED was not provided"),
                    WaitStatus::Continued(_) => unreachable!("WCONTINUED was not provided"),
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    WaitStatus::PtraceEvent(pid, _, _) | WaitStatus::PtraceSyscall(pid) => {
                        job.mark_process(pid, JobStatus::Stopped)
                    }
                    WaitStatus::StillAlive => break,
                }
            }

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
            *foreground = Some(background.remove(i));
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
            foreground_job: Mutex::new(None),
            background_jobs: Mutex::new(Vec::new()),
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
