use std::fmt;

/// Errors the profiler can hit while recording a call tree.
///
/// These are the resource-bound failures: too many distinct nodes, too
/// deep a call stack, a name that is too long, or an `exit()` called
/// with nothing on the stack to close. None of these should ever
/// panic, the profiler is meant to survive being pointed at a program
/// that misbehaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfilerError {
    MaxNodesExceeded { limit: usize },
    MaxDepthExceeded { limit: usize },
    NameTooLong { limit: usize, actual: usize },
    ExitWithoutEnter,
}

impl fmt::Display for ProfilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfilerError::MaxNodesExceeded { limit } => {
                write!(f, "profiler node limit exceeded (max {limit})")
            }
            ProfilerError::MaxDepthExceeded { limit } => {
                write!(f, "profiler depth limit exceeded (max {limit})")
            }
            ProfilerError::NameTooLong { limit, actual } => {
                write!(f, "span name too long ({actual} bytes, max {limit})")
            }
            ProfilerError::ExitWithoutEnter => {
                write!(f, "exit() called with no matching enter()")
            }
        }
    }
}

impl std::error::Error for ProfilerError {}
