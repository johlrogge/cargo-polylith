/// Typed domain errors for `scaffold/` operations.
///
/// These errors are intentionally typed so the module can be treated as an
/// incubating general-purpose crate. Callers in `commands/` and `tui/` use
/// `anyhow::Result`, which implements `From<E: Error + Send + Sync + 'static>`,
/// so `?` propagation works without any changes at call sites.
#[derive(Debug, thiserror::Error)]
pub enum ScaffoldError {
    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("toml error in {path}: {source}")]
    TomlEdit {
        path: std::path::PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{0}")]
    Other(String),
}
