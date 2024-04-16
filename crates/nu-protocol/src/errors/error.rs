use crate::{engine::StateWorkingSet, LabeledError, ParseError, ShellError, Span, Spanned};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    io,
    ops::{Deref, DerefMut},
};

pub type ShellResult<T> = Result<T, Error>;

#[derive(Debug, Clone, Diagnostic, Serialize, Deserialize, PartialEq)]
pub struct Error(Box<ShellError>);

impl Error {
    pub fn wrap(self, working_set: &StateWorkingSet, span: Span) -> ParseError {
        self.0.wrap(working_set, span)
    }

    pub fn into_inner(self) -> ShellError {
        self.into()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

impl Deref for Error {
    type Target = ShellError;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Error {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<ShellError> for Error {
    fn as_ref(&self) -> &ShellError {
        &self.0
    }
}

impl AsMut<ShellError> for Error {
    fn as_mut(&mut self) -> &mut ShellError {
        &mut self.0
    }
}

impl From<Error> for ShellError {
    fn from(error: Error) -> Self {
        *error.0
    }
}

impl From<ShellError> for Error {
    fn from(error: ShellError) -> Self {
        Self(Box::new(error))
    }
}

impl From<Spanned<io::Error>> for Error {
    fn from(error: Spanned<io::Error>) -> Self {
        ShellError::from(error).into()
    }
}

impl From<LabeledError> for Error {
    fn from(error: LabeledError) -> Self {
        ShellError::from(error).into()
    }
}
