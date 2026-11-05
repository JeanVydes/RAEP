#![warn(missing_docs)]

use crate::{AdapterError, BoxFuture};

/// Immutable artifact reference returned by an artifact adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRef {
    /// Opaque adapter-owned identifier.
    pub id: String,
    /// MIME type, if known.
    pub mime_type: Option<String>,
    /// Size in bytes, if known.
    pub bytes: Option<u64>,
}

/// Storage port for uploads, output parts and snapshots.
pub trait ArtifactStore {
    /// Adapter-specific failure.
    type Error: AdapterError;

    /// Stores bytes and returns an opaque reference.
    fn put<'a>(
        &'a mut self,
        mime_type: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<ArtifactRef, Self::Error>>;

    /// Reads an artifact by opaque reference.
    fn get<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, Self::Error>>;
}
