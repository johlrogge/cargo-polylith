use colored::Colorize;

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
