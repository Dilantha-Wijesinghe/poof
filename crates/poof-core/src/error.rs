use std::io;
use std::path::PathBuf;

/// Errors surfaced by `poof-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} is a protected location, so Poof won't remove it", .0.display())]
    Protected(PathBuf),

    #[error("{program} failed: {message}")]
    Command { program: String, message: String },

    #[error("could not read {}: {source}", path.display())]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("the scan was cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Wraps an [`io::Error`] together with what was being attempted and where.
    pub fn io(action: &'static str, path: impl Into<PathBuf>) -> impl FnOnce(io::Error) -> Self {
        let path = path.into();
        move |source| Error::Io { action, path, source }
    }

    /// Whether this error was caused by missing permissions (EPERM / EACCES),
    /// which on macOS usually means Full Disk Access has not been granted.
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Error::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied)
    }
}
