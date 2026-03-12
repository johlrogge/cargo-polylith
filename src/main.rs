mod cli;
mod commands;
mod output;
mod scaffold;
mod workspace;

use clap::Parser;
use cli::{CargoCommand, PolylithCommand};

fn main() {
    let cargo = cli::Cargo::parse();
    let CargoCommand::Polylith(args) = cargo.cmd;

    let workspace_root = args.workspace_root.as_deref();

    let result = match args.command {
        PolylithCommand::Init => commands::init::run(),
        PolylithCommand::Component { action } => {
            use cli::ComponentAction;
            match action {
                ComponentAction::New { name } => {
                    commands::component::new(&name, workspace_root)
                }
            }
        }
        PolylithCommand::Base { action } => {
            use cli::BaseAction;
            match action {
                BaseAction::New { name } => commands::base::new(&name, workspace_root),
            }
        }
        PolylithCommand::Project { action } => {
            use cli::ProjectAction;
            match action {
                ProjectAction::New { name } => commands::project::new(&name, workspace_root),
            }
        }
        PolylithCommand::Deps { component, json } => {
            commands::deps::run(component.as_deref(), json, workspace_root)
        }
        PolylithCommand::Info { json } => commands::info::run(json, workspace_root),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
