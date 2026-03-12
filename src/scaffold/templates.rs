/// Root workspace Cargo.toml `[workspace]` section starter.
pub fn cargo_config_toml() -> &'static str {
    "[build]\ntarget-dir = \"target\"\n"
}

pub fn component_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
"#
    )
}

pub fn component_lib_rs(name: &str) -> String {
    format!(
        r#"mod {name};
pub use {name}::*;
"#
    )
}

pub fn component_impl_rs() -> &'static str {
    "// Implementation goes here\n"
}

pub fn base_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
"#
    )
}

pub fn base_main_rs() -> &'static str {
    "fn main() {\n    println!(\"Hello from base!\");\n}\n"
}

pub fn project_cargo_toml(_name: &str) -> String {
    format!(
        r#"[workspace]
members = [
    # Add base crates here, e.g.:
    # "../../bases/my-base",
]
resolver = "2"

# Use [patch] to swap component implementations:
# [patch.crates-io]
# my-component = {{ path = "../../components/my-component" }}
"#
    )
}
