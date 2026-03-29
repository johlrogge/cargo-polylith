#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("process exited with code {0}")]
    ProcessExit(i32),
}
