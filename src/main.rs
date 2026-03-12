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

    let result = match args.command {
        PolylithCommand::Init => commands::init::run(),
        PolylithCommand::Component { action } => {
            use cli::ComponentAction;
            match action {
                ComponentAction::New { name } => commands::component::new(&name),
            }
        }
        PolylithCommand::Base { action } => {
            use cli::BaseAction;
            match action {
                BaseAction::New { name } => commands::base::new(&name),
            }
        }
        PolylithCommand::Project { action } => {
            use cli::ProjectAction;
            match action {
                ProjectAction::New { name } => commands::project::new(&name),
            }
        }
        PolylithCommand::Deps { component, json } => {
            commands::deps::run(component.as_deref(), json)
        }
        PolylithCommand::Info { json } => commands::info::run(json),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
