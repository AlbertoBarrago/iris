use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("corrupt value in {column}: {value}")]
    Corrupt { column: &'static str, value: String },
    #[error("the database worker stopped")]
    Closed,
    #[error("could not copy the mail store before updating it: {0}")]
    SafetyCopy(String),
    /// A migration failed and the copy taken before it was put back.
    #[error("could not update the mail store to version {version}; the copy at {} was put back: {reason}", .kept.display())]
    Migration {
        version: usize,
        kept: PathBuf,
        reason: String,
    },
    /// A reader that cannot write found the store at another version than
    /// this build knows; the app that writes it has to update it first.
    #[error("the mail store is at version {found}, and this build reads version {wanted}")]
    OtherVersion { found: i64, wanted: usize },
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;
