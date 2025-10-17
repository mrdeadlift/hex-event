use crate::{Event, EventBatch};
use schemars::{schema::RootSchema, schema_for};
use std::fs::File;
use std::io;
use std::path::Path;

/// Return the JSON schema for a single [`Event`].
pub fn event_schema() -> RootSchema {
    schema_for!(Event)
}

/// Return the JSON schema for an [`EventBatch`].
pub fn event_batch_schema() -> RootSchema {
    schema_for!(EventBatch)
}

/// Write the [`EventBatch`] schema to the provided filesystem path.
///
/// The parent directories are created automatically if missing.
pub fn write_event_batch_schema(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &event_batch_schema())
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))
}
