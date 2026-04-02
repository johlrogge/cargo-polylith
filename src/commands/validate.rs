use anyhow::{bail, Result};

pub fn validate_brick_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("name must contain only alphanumeric characters, underscores, or hyphens");
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        bail!("name must start with a letter or underscore");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_are_accepted() {
        assert!(validate_brick_name("my-component").is_ok());
        assert!(validate_brick_name("my_component").is_ok());
        assert!(validate_brick_name("_private").is_ok());
        assert!(validate_brick_name("abc123").is_ok());
    }

    #[test]
    fn empty_name_is_rejected() {
        assert!(validate_brick_name("").is_err());
    }

    #[test]
    fn name_starting_with_digit_is_rejected() {
        assert!(validate_brick_name("1abc").is_err());
    }

    #[test]
    fn name_starting_with_hyphen_is_rejected() {
        assert!(validate_brick_name("-abc").is_err());
    }

    #[test]
    fn name_with_spaces_is_rejected() {
        assert!(validate_brick_name("my component").is_err());
    }

    #[test]
    fn name_with_unicode_is_rejected() {
        assert!(validate_brick_name("caf\u{00e9}").is_err());
    }
}
