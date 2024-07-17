use super::{ControlFlow, InternalError, Interrupted, RuntimeError, ShellError};
use crate::{engine::EngineState, report_error_new};

#[derive(Debug, Clone, PartialEq)]
pub enum Unwind {
    Exiting(i32),
    Interrupted(Interrupted),
    InternalError(InternalError),
    ControlFlow(ControlFlow),
    RuntimeError(RuntimeError),
    ShellError(ShellError),
}

pub type ShellResult<T> = Result<T, Box<Unwind>>;

impl Unwind {
    pub fn report_and_exit(self, engine_state: &EngineState) {
        match self {
            Self::Exiting(code) => std::process::exit(code),
            Self::Interrupted(interrupted) => report_error_new(engine_state, &interrupted),
            Self::InternalError(err) => report_error_new(engine_state, &err),
            Self::ControlFlow(cf) => report_error_new(engine_state, &cf),
            Self::RuntimeError(err) => report_error_new(engine_state, &err),
            Self::ShellError(err) => report_error_new(engine_state, &err),
        };
        std::process::exit(1);
    }
}

impl From<Interrupted> for Unwind {
    fn from(interrupted: Interrupted) -> Self {
        Self::Interrupted(interrupted)
    }
}

impl From<InternalError> for Unwind {
    fn from(error: InternalError) -> Self {
        Self::InternalError(error)
    }
}

impl From<ControlFlow> for Unwind {
    fn from(control_flow: ControlFlow) -> Self {
        Self::ControlFlow(control_flow)
    }
}

impl From<RuntimeError> for Unwind {
    fn from(error: RuntimeError) -> Self {
        Self::RuntimeError(error)
    }
}

impl From<ShellError> for Unwind {
    fn from(error: ShellError) -> Self {
        Self::ShellError(error)
    }
}

impl From<Interrupted> for Box<Unwind> {
    fn from(interrupted: Interrupted) -> Self {
        Box::new(Unwind::Interrupted(interrupted))
    }
}

impl From<InternalError> for Box<Unwind> {
    fn from(error: InternalError) -> Self {
        Box::new(Unwind::InternalError(error))
    }
}

impl From<ControlFlow> for Box<Unwind> {
    fn from(control_flow: ControlFlow) -> Self {
        Box::new(Unwind::ControlFlow(control_flow))
    }
}

impl From<RuntimeError> for Box<Unwind> {
    fn from(error: RuntimeError) -> Self {
        Box::new(Unwind::RuntimeError(error))
    }
}

impl From<ShellError> for Box<Unwind> {
    fn from(error: ShellError) -> Self {
        Box::new(Unwind::ShellError(error))
    }
}

impl From<Box<Interrupted>> for Box<Unwind> {
    fn from(interrupted: Box<Interrupted>) -> Self {
        Self::from(*interrupted)
    }
}

impl From<Box<InternalError>> for Box<Unwind> {
    fn from(error: Box<InternalError>) -> Self {
        Self::from(*error)
    }
}

impl From<Box<ControlFlow>> for Box<Unwind> {
    fn from(control_flow: Box<ControlFlow>) -> Self {
        Self::from(*control_flow)
    }
}

impl From<Box<RuntimeError>> for Box<Unwind> {
    fn from(error: Box<RuntimeError>) -> Self {
        Self::from(*error)
    }
}

impl From<Box<ShellError>> for Box<Unwind> {
    fn from(error: Box<ShellError>) -> Self {
        Self::from(*error)
    }
}
