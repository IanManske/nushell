use crate::Span;
use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error, Diagnostic)]
#[error("Execution interrupted")]
pub struct Interrupted {
    #[label("interrupted here")]
    at: Span,
}
