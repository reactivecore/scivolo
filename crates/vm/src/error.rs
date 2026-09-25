use crate::world::LoadError;
use std::fmt;

#[derive(Debug)]
pub enum VmError {
    Load(LoadError),
    Unsupported(String),
    Uncaught(String),
    Exit(i32),
}

impl From<LoadError> for VmError {
    fn from(e: LoadError) -> Self {
        VmError::Load(e)
    }
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::Load(e) => write!(f, "{e}"),
            VmError::Unsupported(what) => write!(f, "unsupported: {what}"),
            VmError::Uncaught(exc) => write!(f, "uncaught exception: {exc}"),
            VmError::Exit(code) => write!(f, "exit({code})"),
        }
    }
}
