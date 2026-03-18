use colored::Colorize;
use serde::Serialize;
use serde_json::json;

use crate::workspace::check::{Violation, ViolationKind};
use crate::workspace::model::WorkspaceMap;
use crate::workspace::status::StatusReport;

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
    let component_names: std::collections::HashSet<&str> =
        map.components.iter().map(|c| c.name.as_str()).collect();
    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();

    for base in &map.bases {
        if let Some(filter) = filter_component {
            if !base.deps.contains(&filter.to_string()) {
                continue;
            }
        }
        println!("{} (base)", base.name.cyan().bold());
        for dep in &base.deps {
            if component_names.contains(dep.as_str()) {
                println!("  └─ {}", dep.green());
            }
        }
    }

    for project in &map.projects {
        if let Some(filter) = filter_component {
            if !project.deps.contains(&filter.to_string()) {
                continue;
            }
        }
        println!("{} (project)", project.name.yellow().bold());
        for dep in &project.deps {
            if base_names.contains(dep.as_str()) {
                println!("  └─ {} (base)", dep.cyan());
            } else if component_names.contains(dep.as_str()) {
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
            ViolationKind::OrphanComponent      => "orphan".yellow().to_string(),
            ViolationKind::WildcardReExport     => "wildcard".yellow().to_string(),
            ViolationKind::BaseHasMainRs        => "base-has-main".yellow().to_string(),
            ViolationKind::ProjectMissingBase   => "no-base".yellow().to_string(),
            ViolationKind::BaseDepOnBase        => "base-dep-base".red().to_string(),
            ViolationKind::BaseMissingLibRs     => "missing-lib".red().to_string(),
            _                                   => "missing".red().to_string(),
        };
        println!("  [{tag}] {}", v.message);
    }
}

pub fn print_status(report: &StatusReport) {
    if !report.confirmed.is_empty() {
        println!("{}", "Confirmed:".green().bold());
        for item in &report.confirmed {
            println!("  {} {}", "✓".green(), item);
        }
    }

    if !report.divergences.is_empty() {
        println!("{}", "Divergences (not errors):".yellow().bold());
        for d in &report.divergences {
            println!("  {} {}", "~".yellow(), d.observation);
            println!("    {}", d.suggestion);
        }
    }

    if !report.suggestions.is_empty() {
        println!("{}", "Suggestions:".cyan().bold());
        for s in &report.suggestions {
            println!("  {} {}", "→".cyan(), s);
        }
    }

    if report.confirmed.is_empty() && report.divergences.is_empty() && report.suggestions.is_empty()
    {
        println!("{}", "✓ Workspace looks great.".green().bold());
    }
}

pub fn print_status_json(report: &StatusReport) {
    #[derive(Serialize)]
    struct DivergenceOut<'a> {
        observation: &'a str,
        suggestion: &'a str,
    }
    #[derive(Serialize)]
    struct Out<'a> {
        confirmed: &'a [String],
        divergences: Vec<DivergenceOut<'a>>,
        suggestions: &'a [String],
    }
    let out = Out {
        confirmed: &report.confirmed,
        divergences: report
            .divergences
            .iter()
            .map(|d| DivergenceOut { observation: &d.observation, suggestion: &d.suggestion })
            .collect(),
        suggestions: &report.suggestions,
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

pub fn print_check_json(violations: &[Violation]) {
    println!("{}", serde_json::to_string_pretty(&json!({ "violations": violations })).unwrap());
}

pub fn print_deps_json(map: &WorkspaceMap, filter_component: Option<&str>) {
    let component_names: std::collections::HashSet<&str> =
        map.components.iter().map(|c| c.name.as_str()).collect();
    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();

    let bases: Vec<_> = map
        .bases
        .iter()
        .filter(|b| filter_component.map(|f| b.deps.contains(&f.to_string())).unwrap_or(true))
        .map(|b| {
            let component_deps: Vec<&str> = b
                .deps
                .iter()
                .filter(|d| component_names.contains(d.as_str()))
                .map(|d| d.as_str())
                .collect();
            json!({ "name": b.name, "component_deps": component_deps })
        })
        .collect();

    let projects: Vec<_> = map
        .projects
        .iter()
        .filter(|p| filter_component.map(|f| p.deps.contains(&f.to_string())).unwrap_or(true))
        .map(|p| {
            let base_deps: Vec<&str> = p.deps.iter().filter(|d| base_names.contains(d.as_str())).map(|d| d.as_str()).collect();
            let component_deps: Vec<&str> = p.deps.iter().filter(|d| component_names.contains(d.as_str())).map(|d| d.as_str()).collect();
            json!({ "name": p.name, "base_deps": base_deps, "component_deps": component_deps })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json!({ "bases": bases, "projects": projects })).unwrap());
}
