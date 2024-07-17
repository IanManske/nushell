use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum RuntimeError {}
