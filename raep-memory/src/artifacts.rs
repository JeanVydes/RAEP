#![warn(missing_docs)]

use raep_ports::{ArtifactRef, ArtifactStore};
use std::collections::HashMap;
use std::convert::Infallible;

/// In-memory artifact store.
#[derive(Clone, Debug, Default)]
pub struct MemoryArtifactStore {
    artifacts: HashMap<String, (String, Vec<u8>)>,
    next_id: u64,
}

impl ArtifactStore for MemoryArtifactStore {
    type Error = Infallible;

    fn put<'a>(
        &'a mut self,
        mime_type: &'a str,
        bytes: &'a [u8],
    ) -> raep_ports::BoxFuture<'a, Result<ArtifactRef, Self::Error>> {
        self.next_id += 1;
        let id = format!("artifact-{}", self.next_id);
        self.artifacts
            .insert(id.clone(), (mime_type.to_owned(), bytes.to_vec()));
        Box::pin(async move {
            Ok(ArtifactRef {
                id,
                mime_type: Some(mime_type.to_owned()),
                bytes: Some(bytes.len() as u64),
            })
        })
    }

    fn get<'a>(
        &'a self,
        id: &'a str,
    ) -> raep_ports::BoxFuture<'a, Result<Option<Vec<u8>>, Self::Error>> {
        Box::pin(async move { Ok(self.artifacts.get(id).map(|(_, bytes)| bytes.clone())) })
    }
}
