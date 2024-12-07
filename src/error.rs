use thiserror::Error;

#[derive(Error, Debug)]
pub enum HudError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("API error: {0}")]
    Api(String),

    // #[error("Cache error: {0}")]
    // Cache(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
