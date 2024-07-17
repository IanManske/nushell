use crate::{Span, Value};
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum ControlFlow {
    #[error("Break used outside of loop")]
    Break {
        #[label("not inside a loop")]
        span: Span,
    },

    #[error("Continue used outside of loop")]
    Continue {
        #[label("not inside a loop")]
        span: Span,
    },

    #[error("Return used outside of function")]
    Return {
        #[label("not inside a function")]
        span: Span,
        value: Box<Value>,
    },
}
