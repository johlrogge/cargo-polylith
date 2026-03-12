use colored::Colorize;
use serde::Serialize;
use serde_json::json;

use crate::workspace::check::{Violation, ViolationKind};
use crate::workspace::model::WorkspaceMap;

pub fn print_info(map: &WorkspaceMap) {
    println!("{}", "Components".bold());
    if map.components.is_empty() {
        println!("  (none)");
    } else {
        for c in &map.components {
            println!("  {}", c.name.green());
        }
    }

    println!("{}", "Bases".bold());
    if map.bases.is_empty() {
        println!("  (none)");
    } else {
        for b in &map.bases {
            println!("  {}", b.name.cyan());
        }
    }

    println!("{}", "Projects".bold());
    if map.projects.is_empty() {
        println!("  (none)");
    } else {
        for p in &map.projects {
            println!("  {}", p.name.yellow());
        }
    }
}

pub fn print_info_json(map: &WorkspaceMap) {
    #[derive(Serialize)]
    struct BrickOut<'a> {
        name: &'a str,
        deps: &'a [String],
    }
    #[derive(Serialize)]
    struct ProjectOut<'a> {
        name: &'a str,
    }
    #[derive(Serialize)]
    struct InfoOut<'a> {
        components: Vec<BrickOut<'a>>,
        bases: Vec<BrickOut<'a>>,
        projects: Vec<ProjectOut<'a>>,
    }

    let out = InfoOut {
        components: map
            .components
            .iter()
            .map(|b| BrickOut { name: &b.name, deps: &b.deps })
            .collect(),
        bases: map
            .bases
            .iter()
            .map(|b| BrickOut { name: &b.name, deps: &b.deps })
            .collect(),
        projects: map
            .projects
            .iter()
            .map(|p| ProjectOut { name: &p.name })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

pub fn print_deps(map: &WorkspaceMap, filter_component: Option<&str>) {
    for base in &map.bases {
        if let Some(filter) = filter_component {
            if !base.deps.contains(&filter.to_string()) {
                continue;
            }
        }
        println!("{} (base)", base.name.cyan().bold());
        for dep in &base.deps {
            let is_component = map.components.iter().any(|c| &c.name == dep);
            if is_component {
                println!("  └─ {}", dep.green());
            }
        }
    }
}

pub fn print_check(violations: &[Violation]) {
    if violations.is_empty() {
        println!("{}", "✓ No violations found.".green().bold());
        return;
    }
    println!("{}", format!("{} violation(s) found:", violations.len()).red().bold());
    for v in violations {
        let tag = match v.kind {
            ViolationKind::OrphanComponent => "orphan".yellow().to_string(),
            ViolationKind::BaseDepOnBase => "base-dep-base".red().to_string(),
            _ => "missing".red().to_string(),
        };
        println!("  [{tag}] {}", v.message);
    }
}

pub fn print_check_json(violations: &[Violation]) {
    println!("{}", serde_json::to_string_pretty(&json!({ "violations": violations })).unwrap());
}

pub fn print_deps_json(map: &WorkspaceMap, filter_component: Option<&str>) {
    let bases: Vec<_> = map
        .bases
        .iter()
        .filter(|b| {
            filter_component
                .map(|f| b.deps.contains(&f.to_string()))
                .unwrap_or(true)
        })
        .map(|b| {
            let component_deps: Vec<&str> = b
                .deps
                .iter()
                .filter(|d| map.components.iter().any(|c| &c.name == *d))
                .map(|d| d.as_str())
                .collect();
            json!({ "name": b.name, "component_deps": component_deps })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json!({ "bases": bases })).unwrap());
}
