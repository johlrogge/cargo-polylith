use std::env;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::scaffold::templates;
use crate::workspace::resolve_root;

pub fn skill(workspace_root: Option<&Path>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;

    let commands_dir = root.join(".claude").join("commands");
    fs::create_dir_all(&commands_dir)
        .with_context(|| format!("creating {}", commands_dir.display()))?;

    let skill_path = commands_dir.join("polylith.md");
    fs::write(&skill_path, templates::claude_skill_md())
        .with_context(|| format!("writing {}", skill_path.display()))?;

    println!("wrote {}", skill_path.display());

    let devenv = root.join("devenv.nix").exists();
    print_mcp_hint(&root, devenv);

    Ok(())
}

fn print_mcp_hint(root: &Path, devenv: bool) {
    let (command, args) = if devenv {
        (
            "devenv",
            r#"["shell", "--", "cargo-polylith", "polylith", "mcp", "serve"]"#,
        )
    } else {
        ("cargo-polylith", r#"["polylith", "mcp", "serve"]"#)
    };

    let mcp_json = root.join(".mcp.json");
    let managed = mcp_json.exists() && fs::symlink_metadata(&mcp_json)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);

    println!();
    if managed {
        println!(
            "note: .mcp.json is a managed symlink — add the cargo-polylith server via your \
             devenv.nix or equivalent config:"
        );
    } else {
        println!("add the MCP server to .mcp.json:");
    }
    println!(
        r#"  "cargo-polylith": {{
    "command": "{command}",
    "args": {args},
    "type": "stdio"
  }}"#
    );
}
