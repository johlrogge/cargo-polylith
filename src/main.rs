mod cli;
mod commands;
mod corsett;
mod output;
mod scaffold;
mod tui;
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
                ComponentAction::New { name, interface } => {
                    commands::component::new(&name, interface.as_deref(), workspace_root)
                }
                ComponentAction::Update { name, interface } => {
                    commands::component::update(&name, interface.as_deref(), workspace_root)
                }
            }
        }
        PolylithCommand::Base { action } => {
            use cli::BaseAction;
            match action {
                BaseAction::New { name } => commands::base::new(&name, workspace_root),
                BaseAction::Update { name, test_base } => {
                    commands::base::update(&name, test_base, workspace_root)
                }
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
        PolylithCommand::Check { json, profile } => {
            commands::check::run(json, profile.as_deref(), workspace_root)
        }
        PolylithCommand::Status { json } => commands::status::run(json, workspace_root),
        PolylithCommand::Edit => commands::edit::run(workspace_root),
        PolylithCommand::Generate { action } => {
            use cli::GenerateAction;
            match action {
                GenerateAction::Skill => commands::generate::skill(workspace_root),
            }
        }
        PolylithCommand::Mcp { action } => {
            use cli::McpAction;
            match action {
                McpAction::Serve { write } => commands::mcp::serve(workspace_root, write),
            }
        }
        PolylithCommand::Profile { action } => {
            use cli::ProfileAction;
            match action {
                ProfileAction::New { name } => commands::profile::new(&name, workspace_root),
                ProfileAction::List { json } => commands::profile::list(json, workspace_root),
                ProfileAction::Build { name, no_build } => {
                    commands::profile::build(&name, no_build, workspace_root)
                }
                ProfileAction::Add { interface, r#impl, profile } => {
                    commands::profile::add(&interface, &r#impl, &profile, workspace_root)
                }
                ProfileAction::Migrate { force } => commands::profile::migrate(force, workspace_root),
            }
        }
        PolylithCommand::Cargo { profile, args: cargo_args } => {
            let profile_name = profile.as_deref().unwrap_or("dev");
            commands::profile::run_cargo(profile_name, &cargo_args, workspace_root)
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
