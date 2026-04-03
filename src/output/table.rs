use colored::Colorize;
use serde::Serialize;
use serde_json::json;

use crate::workspace::check::{Violation, ViolationKind};
use crate::workspace::model::{Profile, WorkspaceMap};
use crate::workspace::status::StatusReport;
use crate::workspace::{classify_dep, DepKind};

pub fn print_info(map: &WorkspaceMap) {
    println!("{}", "Components".bold());
    if map.components.is_empty() {
        println!("  (none)");
    } else {
        let has_interfaces = map.components.iter().any(|c| c.interface.is_some());
        if has_interfaces {
            const IFACE_W: usize = 18;
            let mut sorted: Vec<_> = map.components.iter().collect();
            sorted.sort_by(|a, b| match (&a.interface, &b.interface) {
                (Some(ai), Some(bi)) => ai.cmp(bi).then(a.name.cmp(&b.name)),
                (Some(_), None)      => std::cmp::Ordering::Less,
                (None, Some(_))      => std::cmp::Ordering::Greater,
                (None, None)         => a.name.cmp(&b.name),
            });
            let mut prev_iface: Option<&str> = None;
            for comp in &sorted {
                let iface = comp.interface.as_deref();
                let iface_label = if iface != prev_iface { iface.unwrap_or("") } else { "" };
                println!(
                    "  {:<IFACE_W$}{}",
                    iface_label.dimmed(),
                    comp.name.green()
                );
                prev_iface = iface;
            }
        } else {
            for c in &map.components {
                println!("  {}", c.name.green());
            }
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
            match classify_dep(dep, map) {
                DepKind::Base(name)      => println!("  └─ {} (base)", name.cyan()),
                DepKind::Interface(name) => println!("  └─ {}", name.green()),
                DepKind::External        => {}
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
            match classify_dep(dep, map) {
                DepKind::Base(name)      => println!("  └─ {} (base)", name.cyan()),
                DepKind::Interface(name) => println!("  └─ {}", name.green()),
                DepKind::External        => {}
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
        let tag = match &v.kind {
            ViolationKind::OrphanComponent { .. }      => "orphan".yellow().to_string(),
            ViolationKind::WildcardReExport { .. }     => "wildcard".yellow().to_string(),
            ViolationKind::BaseHasMainRs { .. }        => "base-has-main".yellow().to_string(),
            ViolationKind::ProjectMissingBase { .. }   => "no-base".yellow().to_string(),
            ViolationKind::NotInRootWorkspace { .. }   => "not-in-workspace".yellow().to_string(),
            ViolationKind::AmbiguousInterface { .. }   => "ambiguous-interface".yellow().to_string(),
            ViolationKind::DuplicateName { .. }        => "duplicate-name".yellow().to_string(),
            ViolationKind::MissingInterface { .. }     => "missing-interface".yellow().to_string(),
            ViolationKind::BaseMissingLibRs { .. }     => "missing-lib".red().to_string(),
            ViolationKind::MissingLibRs { .. }         => "missing-lib".red().to_string(),
            ViolationKind::MissingImplFile { .. }      => "missing-impl".red().to_string(),
            ViolationKind::DepKeyMismatch { .. }       => "dep-key-mismatch".red().to_string(),
            ViolationKind::ProjectFeatureDrift { .. }  => "project-feature-drift".yellow().to_string(),
            ViolationKind::ProjectVersionDrift { .. }  => "project-version-drift".yellow().to_string(),
            ViolationKind::ProjectNotInRootWorkspace { .. } => "project-not-in-workspace".red().to_string(),
            ViolationKind::ProjectHasOwnWorkspace { .. } => "project-has-own-workspace".red().to_string(),
            ViolationKind::ProfileImplPathNotFound { .. } => "profile-impl-not-found".red().to_string(),
            ViolationKind::ProfileImplNotAComponent { .. } => "profile-impl-not-component".red().to_string(),
            ViolationKind::HardwiredDep { .. }         => "hardwired-dep".yellow().to_string(),
            ViolationKind::HardwiredImplDep { .. }     => "hardwired-impl".red().to_string(),
            ViolationKind::BrickNotUsingWorkspaceVersion { .. } => "not-workspace-version".yellow().to_string(),
        };
        println!("  [{tag}] {}", v.kind);
    }
}

pub fn print_profiles(profiles: &[Profile]) {
    if profiles.is_empty() {
        println!("No profiles found. Create profiles/<name>.profile to define a profile.");
        return;
    }
    println!("{}", "Profiles".bold());
    for profile in profiles {
        println!("  {}", profile.name.cyan().bold());
        if profile.implementations.is_empty() && profile.libraries.is_empty() {
            println!("    (empty)");
        }
        if !profile.implementations.is_empty() {
            println!("    {}", "Implementations:".dimmed());
            let mut entries: Vec<_> = profile.implementations.iter().collect();
            entries.sort_by_key(|(k, _)| k.as_str());
            for (iface, path) in entries {
                println!("      {} \u{2192} {}", iface.green(), path);
            }
        }
        if !profile.libraries.is_empty() {
            println!("    {}", "Libraries:".dimmed());
            let mut entries: Vec<_> = profile.libraries.iter().collect();
            entries.sort_by_key(|(k, _)| k.as_str());
            for (key, info) in entries {
                match (&info.version, info.features.is_empty()) {
                    (Some(v), true) => println!("      {} = \"{}\"", key, v),
                    (Some(v), false) => println!("      {} = {{ version = \"{}\", features = {:?} }}", key, v, info.features),
                    (None, false) => println!("      {} = {{ features = {:?} }}", key, info.features),
                    (None, true) => {} // nothing to show
                }
            }
        }
    }
}

pub fn print_profiles_json(profiles: &[Profile]) {
    let out: Vec<_> = profiles
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "implementations": p.implementations,
                "libraries": p.libraries.iter().map(|(k, v)| {
                    (k.clone(), serde_json::json!({ "version": v.version, "features": v.features }))
                }).collect::<std::collections::HashMap<_, _>>()
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "profiles": out })).unwrap());
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
    let bases: Vec<_> = map
        .bases
        .iter()
        .filter(|b| filter_component.map(|f| b.deps.contains(&f.to_string())).unwrap_or(true))
        .map(|b| {
            let mut base_deps: Vec<&str> = vec![];
            let mut component_deps: Vec<&str> = vec![];
            for dep in &b.deps {
                match classify_dep(dep, map) {
                    DepKind::Base(name)      => base_deps.push(name),
                    DepKind::Interface(name) => component_deps.push(name),
                    DepKind::External        => {}
                }
            }
            json!({ "name": b.name, "base_deps": base_deps, "component_deps": component_deps })
        })
        .collect();

    let projects: Vec<_> = map
        .projects
        .iter()
        .filter(|p| filter_component.map(|f| p.deps.contains(&f.to_string())).unwrap_or(true))
        .map(|p| {
            let mut base_deps: Vec<&str> = vec![];
            let mut component_deps: Vec<&str> = vec![];
            for dep in &p.deps {
                match classify_dep(dep, map) {
                    DepKind::Base(name)      => base_deps.push(name),
                    DepKind::Interface(name) => component_deps.push(name),
                    DepKind::External        => {}
                }
            }
            json!({ "name": p.name, "base_deps": base_deps, "component_deps": component_deps })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&json!({ "bases": bases, "projects": projects })).unwrap());
}
