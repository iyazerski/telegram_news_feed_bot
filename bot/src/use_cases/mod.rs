pub mod manage_feed;
pub mod manage_recaps;

/// A management request that failed.
#[derive(Debug, thiserror::Error)]
pub enum ManageError {
    /// The request asks for something the feed cannot do; the message tells the user why.
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}
