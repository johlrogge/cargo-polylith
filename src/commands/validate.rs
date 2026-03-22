use anyhow::{bail, Result};

pub fn validate_brick_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("name must contain only alphanumeric characters, underscores, or hyphens");
    }
    Ok(())
}
