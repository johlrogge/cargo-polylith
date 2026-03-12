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
}

#[derive(Subcommand)]
pub enum ComponentAction {
    /// Create a new component
    New {
        /// Name of the component (snake_case)
        name: String,
    },
}

#[derive(Subcommand)]
pub enum BaseAction {
    /// Create a new base
    New {
        /// Name of the base (snake_case)
        name: String,
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
