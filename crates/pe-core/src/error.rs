//! Error taxonomy for `pe-core`.
//!
//! Library crates return typed errors; only `pe-app` collapses them into an
//! application error at the IPC boundary, where the action and its subject
//! are attached (`Context::doing`).

use thiserror::Error;

/// Errors produced by the document model and project file I/O.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A project file declared a schema version this build cannot read.
    #[error(
        "this project was written by a newer version of PolarExplorer (file schema version {found}; this build reads up to version {supported})"
    )]
    SchemaTooNew {
        /// The version in the file.
        found: u32,
        /// The newest version this build reads.
        supported: u32,
    },

    /// A migration could not upgrade a document.
    #[error("could not upgrade the project from schema version {from}: {reason}")]
    Migration {
        /// The version being migrated from.
        from: u32,
        /// What went wrong.
        reason: String,
    },

    /// Reading or writing a project file failed.
    #[error("the project file could not be read or written: {0}")]
    Io(#[from] std::io::Error),

    /// The project JSON could not be parsed or written.
    #[error("the project file is damaged and could not be read: {0}")]
    Json(#[from] serde_json::Error),

    /// The `.wpsproj` container was not a readable archive, or held something
    /// a project archive never holds.
    #[error("the project file is not a valid project archive: {0}")]
    Archive(String),

    /// The document broke one of its own rules.
    #[error("the project is not valid: {0}")]
    Invalid(String),

    /// A command referred to a source that is not in the project.
    #[error(
        "that source is no longer in the project (#{0}); it may have been removed, or an undo may have taken it back"
    )]
    MissingSource(u64),

    /// A command referred to a position outside the source list.
    #[error("position {index} is outside the source list of length {len}")]
    IndexOutOfBounds {
        /// The offending index.
        index: usize,
        /// The list's length.
        len: usize,
    },

    /// A command found the document in a state other than the one it was
    /// built against, so applying it would not be the change it describes.
    #[error("the project changed underneath this edit: {0}")]
    Stale(String),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, CoreError>;
