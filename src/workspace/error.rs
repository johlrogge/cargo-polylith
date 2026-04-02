/// Typed domain errors for `workspace/` operations.
///
/// These errors are intentionally typed so the module can be treated as an
/// incubating general-purpose crate. Callers in `commands/` and `tui/` use
/// `anyhow::Result`, which implements `From<E: Error + Send + Sync + 'static>`,
/// so `?` propagation works without any changes at call sites.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    TomlParse {
        path: std::path::PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{0}")]
    Other(String),
}
