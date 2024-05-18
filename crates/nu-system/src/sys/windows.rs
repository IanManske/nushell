use std::{
    io,
    process::{Child, Command, ExitStatus},
};

#[derive(Debug, Clone)]
pub struct ForegroundState {}

impl ForegroundState {
    pub fn new() -> Self {
        Self {}
    }

    pub unsafe fn force_reset(&self) {}
}

pub struct ForegroundChild {
    child: Child,
}

impl ForegroundChild {
    pub fn spawn(
        mut command: Command,
        _interactive: bool,
        _state: &ForegroundState,
    ) -> io::Result<Self> {
        command.spawn().map(|child| Self { child })
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.as_mut().wait()
    }
}

impl AsMut<Child> for ForegroundChild {
    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

#[derive(Debug)]
pub struct ForegroundGuard {}

impl ForegroundGuard {
    pub fn new(_pid: u32, _foreground_state: &ForegroundState) -> io::Result<ForegroundGuard> {
        Ok(ForegroundGuard {})
    }

    pub fn pgroup(&self) -> Option<u32> {
        None
    }
}
