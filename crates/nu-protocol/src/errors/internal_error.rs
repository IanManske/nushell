use crate::Span;
use miette::Diagnostic;
use thiserror::Error;

/// A catastrophic Nushell failure. This reflects a completely unexpected or unrecoverable error.
///
/// Only use this one if Nushell completely falls over and hits a state that isn't possible or isn't recoverable.
#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum InternalError {
    #[error("Nushell failed: {msg}.")]
    #[diagnostic(
        code(nu::shell::nushell_failed),
        help(
        "This shouldn't happen. Please file an issue: https://github.com/nushell/nushell/issues"
    ))]
    Message { msg: String },

    #[error("Nushell failed: {msg}.")]
    #[diagnostic(
        code(nu::shell::nushell_failed_spanned),
        help(
        "This shouldn't happen. Please file an issue: https://github.com/nushell/nushell/issues"
    ))]
    Spanned {
        msg: String,
        label: String,
        #[label = "{label}"]
        span: Span,
    },

    #[error("Nushell failed: {msg}.")]
    #[diagnostic(code(nu::shell::nushell_failed_help))]
    Help {
        msg: String,
        #[help]
        help: String,
    },
}
