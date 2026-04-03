use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(bin_name = "cargo")]
pub struct Cargo {
    #[command(subcommand)]
    pub cmd: CargoCommand,
}

#[derive(Subcommand)]
pub enum CargoCommand {
    /// Polylith architecture tools for Cargo workspaces
    Polylith(PolylithArgs),
}

#[derive(Args)]
pub struct PolylithArgs {
    /// Override the workspace root (defaults to walking up from the current directory)
    #[arg(long, global = true, value_name = "PATH")]
    pub workspace_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: PolylithCommand,
}

#[derive(Subcommand)]
pub enum PolylithCommand {
    /// Initialise a Cargo workspace as a polylith monorepo
    Init,
    /// Manage components
    Component {
        #[command(subcommand)]
        action: ComponentAction,
    },
    /// Manage bases
    Base {
        #[command(subcommand)]
        action: BaseAction,
    },
    /// Manage projects
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Show the dependency graph between bases and components
    Deps {
        /// Show only bases that depend on this component
        #[arg(long, value_name = "NAME")]
        component: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show workspace info: components, bases, and projects
    Info {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Check workspace structure for violations
    Check {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Also validate this profile (name without .profile extension)
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
    },
    /// Show a lenient audit of workspace structure (always exits 0)
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Interactively compose projects (TUI)
    Edit,
    /// Generate Claude Code integration files
    Generate {
        #[command(subcommand)]
        action: GenerateAction,
    },
    /// MCP protocol server
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Manage polylith profiles (named sets of implementation selections)
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Bump the workspace version (relaxed mode only)
    Bump {
        /// Bump level: major, minor, or patch
        level: String,
    },
    /// Run a cargo command against a generated profile workspace
    Cargo {
        /// Profile name to activate (defaults to "dev")
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
        /// Cargo subcommand and arguments (e.g. build, test, clippy -- -D warnings)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum GenerateAction {
    /// Write .claude/commands/polylith.md skill file for Claude Code
    Skill,
}

#[derive(Subcommand)]
pub enum McpAction {
    /// Start the MCP server (JSON-RPC 2.0 over stdin/stdout)
    Serve {
        /// Enable write tools (scaffold components/bases/projects, patch implementations)
        #[arg(long)]
        write: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// Create a new empty profile
    New {
        /// Profile name (without .profile extension)
        name: String,
    },
    /// List all profiles and their implementation selections
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Generate a profile workspace Cargo.toml and build it
    Build {
        /// Profile name (without .profile extension)
        name: String,
        /// Generate the workspace Cargo.toml but do not invoke cargo build
        #[arg(long)]
        no_build: bool,
    },
    /// Add or update an implementation selection in a profile
    Add {
        /// Interface name (the dep key in [workspace.dependencies])
        interface: String,
        /// Path to the implementation component (relative to workspace root)
        #[arg(long, value_name = "PATH")]
        r#impl: String,
        /// Profile name to update (without .profile extension)
        #[arg(long, value_name = "NAME")]
        profile: String,
    },
    /// Migrate root workspace members into a dev profile
    Migrate {
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ComponentAction {
    /// Create a new component
    New {
        /// Name of the component (snake_case)
        name: String,
        /// Interface name (defaults to the crate name)
        #[arg(long, value_name = "NAME")]
        interface: Option<String>,
    },
    /// Set or update the interface annotation on an existing component
    Update {
        /// Name of the component
        name: String,
        /// Interface name (defaults to the crate name)
        #[arg(long, value_name = "NAME")]
        interface: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum BaseAction {
    /// Create a new base
    New {
        /// Name of the base (snake_case)
        name: String,
    },
    /// Update metadata on an existing base
    Update {
        /// Name of the base
        name: String,
        /// Mark this base as a test-base
        #[arg(long, default_value_t = false)]
        test_base: bool,
    },
}

#[derive(Subcommand)]
pub enum ProjectAction {
    /// Create a new project
    New {
        /// Name of the project
        name: String,
    },
}
