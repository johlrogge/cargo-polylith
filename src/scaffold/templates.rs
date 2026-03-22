/// Root workspace Cargo.toml `[workspace]` section starter.
pub fn cargo_config_toml() -> &'static str {
    "[build]\ntarget-dir = \"target\"\n"
}

pub fn component_cargo_toml(name: &str, interface: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [package.metadata.polylith]\ninterface = \"{interface}\"\n\n[dependencies]\n"
    )
}

pub fn component_lib_rs(name: &str) -> String {
    format!(
        r#"mod {name};
// Declare your public interface explicitly, e.g.:
// pub use {name}::{{MyType, my_function}};
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

pub fn base_lib_rs() -> &'static str {
    "pub fn run() {}\n"
}

pub fn claude_skill_md() -> &'static str {
    include_str!("claude_skill.md")
}

pub fn project_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
# Add base and component deps here, e.g.:
# my-base = {{ path = "../../bases/my-base" }}
# my-component-stub = {{ path = "../../components/my-component-stub", package = "my-component-stub" }}
"#
    )
}
