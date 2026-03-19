use std::env;
use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::scaffold;
use crate::workspace::{build_workspace_map, resolve_root, run_checks, run_status};

pub fn serve(workspace_root: Option<&Path>, write: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = resolve_root(&cwd, workspace_root)?;

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req["method"].as_str().unwrap_or("");

        let response = match method {
            "initialize" => initialize(id),
            "initialized" => continue,
            "tools/list" => tools_list(id, write),
            "tools/call" => tools_call(id, &req, &root, write),
            _ => method_not_found(id, method),
        };

        writeln!(out, "{}", serde_json::to_string(&response)?)?;
        out.flush()?;
    }
    Ok(())
}

fn initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "cargo-polylith",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {}
            }
        }
    })
}

fn tools_list(id: Value, write: bool) -> Value {
    let mut tools = vec![
        json!({
            "name": "polylith_info",
            "description": "Return workspace info: all components, bases, and projects with their deps",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "polylith_deps",
            "description": "Return the dependency graph between bases, projects, and components",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "component": {
                        "type": "string",
                        "description": "Filter to show only bases/projects that depend on this component"
                    }
                }
            }
        }),
        json!({
            "name": "polylith_check",
            "description": "Check workspace structure for polylith violations",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "polylith_status",
            "description": "Show a lenient audit of workspace structure (divergences and suggestions)",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ];

    if write {
        tools.extend([
            json!({
                "name": "polylith_component_new",
                "description": "Create a new component under components/<name>/",
                "inputSchema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "Component name (snake_case)" },
                        "interface": { "type": "string", "description": "Interface group name (defaults to component name)" }
                    }
                }
            }),
            json!({
                "name": "polylith_base_new",
                "description": "Create a new base under bases/<name>/",
                "inputSchema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "Base name (snake_case)" }
                    }
                }
            }),
            json!({
                "name": "polylith_project_new",
                "description": "Create a new project workspace under projects/<name>/",
                "inputSchema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "Project name" }
                    }
                }
            }),
            json!({
                "name": "polylith_component_update",
                "description": "Set or update the interface annotation on an existing component",
                "inputSchema": {
                    "type": "object",
                    "required": ["name", "interface"],
                    "properties": {
                        "name": { "type": "string", "description": "Component name" },
                        "interface": { "type": "string", "description": "Interface group name" }
                    }
                }
            }),
            json!({
                "name": "polylith_set_implementation",
                "description": "Select which component implementation to use for an interface in a project, by writing a [dependencies] entry with path (and package = if the crate name differs from the interface name)",
                "inputSchema": {
                    "type": "object",
                    "required": ["project", "interface", "implementation"],
                    "properties": {
                        "project": { "type": "string", "description": "Project name" },
                        "interface": { "type": "string", "description": "Interface (crate name) to patch" },
                        "implementation": { "type": "string", "description": "Component name providing the implementation" }
                    }
                }
            }),
        ]);
    }

    json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } })
}

fn tools_call(id: Value, req: &Value, root: &Path, write: bool) -> Value {
    let params = &req["params"];
    let name = params["name"].as_str().unwrap_or("");
    let arguments = &params["arguments"];

    let result_text = match name {
        // ── read tools ──────────────────────────────────────────────────────
        "polylith_info" => match build_workspace_map(root) {
            Ok(map) => {
                #[derive(serde::Serialize)]
                struct BrickOut<'a> {
                    name: &'a str,
                    deps: &'a [String],
                    interface: Option<&'a str>,
                }
                #[derive(serde::Serialize)]
                struct ProjectOut<'a> {
                    name: &'a str,
                    deps: &'a [String],
                }
                #[derive(serde::Serialize)]
                struct InfoOut<'a> {
                    components: Vec<BrickOut<'a>>,
                    bases: Vec<BrickOut<'a>>,
                    projects: Vec<ProjectOut<'a>>,
                }
                let out = InfoOut {
                    components: map.components.iter().map(|b| BrickOut {
                        name: &b.name,
                        deps: &b.deps,
                        interface: b.interface.as_deref(),
                    }).collect(),
                    bases: map.bases.iter().map(|b| BrickOut {
                        name: &b.name,
                        deps: &b.deps,
                        interface: b.interface.as_deref(),
                    }).collect(),
                    projects: map.projects.iter().map(|p| ProjectOut {
                        name: &p.name,
                        deps: &p.deps,
                    }).collect(),
                };
                serde_json::to_string_pretty(&out).unwrap_or_else(|e| e.to_string())
            }
            Err(e) => format!("error: {e:#}"),
        },

        "polylith_deps" => {
            let filter = arguments.get("component").and_then(|v| v.as_str());
            match build_workspace_map(root) {
                Ok(map) => {
                    let component_names: std::collections::HashSet<&str> =
                        map.components.iter().map(|c| c.name.as_str()).collect();
                    let base_names: std::collections::HashSet<&str> =
                        map.bases.iter().map(|b| b.name.as_str()).collect();

                    let bases: Vec<_> = map
                        .bases
                        .iter()
                        .filter(|b| filter.map(|f| b.deps.iter().any(|d| d == f)).unwrap_or(true))
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
                        .filter(|p| filter.map(|f| p.deps.iter().any(|d| d == f)).unwrap_or(true))
                        .map(|p| {
                            let base_deps: Vec<&str> = p
                                .deps
                                .iter()
                                .filter(|d| base_names.contains(d.as_str()))
                                .map(|d| d.as_str())
                                .collect();
                            let component_deps: Vec<&str> = p
                                .deps
                                .iter()
                                .filter(|d| component_names.contains(d.as_str()))
                                .map(|d| d.as_str())
                                .collect();
                            json!({ "name": p.name, "base_deps": base_deps, "component_deps": component_deps })
                        })
                        .collect();

                    serde_json::to_string_pretty(
                        &json!({ "bases": bases, "projects": projects }),
                    )
                    .unwrap_or_else(|e| e.to_string())
                }
                Err(e) => format!("error: {e:#}"),
            }
        }

        "polylith_check" => match build_workspace_map(root) {
            Ok(map) => {
                let violations = run_checks(&map);
                serde_json::to_string_pretty(&json!({ "violations": violations }))
                    .unwrap_or_else(|e| e.to_string())
            }
            Err(e) => format!("error: {e:#}"),
        },

        "polylith_status" => match build_workspace_map(root) {
            Ok(map) => {
                let report = run_status(&map);
                #[derive(serde::Serialize)]
                struct DivergenceOut<'a> {
                    observation: &'a str,
                    suggestion: &'a str,
                }
                #[derive(serde::Serialize)]
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
                        .map(|d| DivergenceOut {
                            observation: &d.observation,
                            suggestion: &d.suggestion,
                        })
                        .collect(),
                    suggestions: &report.suggestions,
                };
                serde_json::to_string_pretty(&out).unwrap_or_else(|e| e.to_string())
            }
            Err(e) => format!("error: {e:#}"),
        },

        // ── write tools ─────────────────────────────────────────────────────
        "polylith_component_new" | "polylith_base_new" | "polylith_project_new"
        | "polylith_component_update" | "polylith_set_implementation"
            if !write =>
        {
            "write tools disabled — restart the MCP server with --write to enable scaffolding"
                .to_string()
        }

        "polylith_component_new" => {
            let comp_name = arguments["name"].as_str().unwrap_or("");
            let interface = arguments
                .get("interface")
                .and_then(|v| v.as_str())
                .unwrap_or(comp_name);
            match scaffold::create_component(root, comp_name, interface) {
                Ok(()) => format!("created component '{comp_name}' with interface '{interface}'"),
                Err(e) => format!("error: {e:#}"),
            }
        }

        "polylith_base_new" => {
            let base_name = arguments["name"].as_str().unwrap_or("");
            match scaffold::create_base(root, base_name) {
                Ok(()) => format!("created base '{base_name}'"),
                Err(e) => format!("error: {e:#}"),
            }
        }

        "polylith_project_new" => {
            let project_name = arguments["name"].as_str().unwrap_or("");
            match scaffold::create_project(root, project_name) {
                Ok(()) => format!("created project '{project_name}'"),
                Err(e) => format!("error: {e:#}"),
            }
        }

        "polylith_component_update" => {
            let comp_name = arguments["name"].as_str().unwrap_or("");
            let interface = arguments["interface"].as_str().unwrap_or("");
            match build_workspace_map(root) {
                Ok(map) => {
                    match map.components.iter().find(|c| c.name == comp_name) {
                        Some(comp) => {
                            match scaffold::write_interface_to_toml(&comp.path, interface) {
                                Ok(()) => format!("updated component '{comp_name}' interface to '{interface}'"),
                                Err(e) => format!("error: {e:#}"),
                            }
                        }
                        None => format!("component '{comp_name}' not found in workspace"),
                    }
                }
                Err(e) => format!("error: {e:#}"),
            }
        }

        "polylith_set_implementation" => {
            let project_name = arguments["project"].as_str().unwrap_or("");
            let interface = arguments["interface"].as_str().unwrap_or("");
            let impl_name = arguments["implementation"].as_str().unwrap_or("");
            match build_workspace_map(root) {
                Ok(map) => {
                    let project = map.projects.iter().find(|p| p.name == project_name);
                    let component = map.components.iter().find(|c| c.name == impl_name);
                    match (project, component) {
                        (Some(proj), Some(comp)) => {
                            match scaffold::set_project_implementation(&proj.path, interface, &comp.path) {
                                Ok(()) => format!(
                                    "set implementation of '{interface}' to '{impl_name}' in project '{project_name}'"
                                ),
                                Err(e) => format!("error: {e:#}"),
                            }
                        }
                        (None, _) => format!("project '{project_name}' not found"),
                        (_, None) => format!("component '{impl_name}' not found"),
                    }
                }
                Err(e) => format!("error: {e:#}"),
            }
        }

        _ => format!("unknown tool: {name}"),
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": result_text }]
        }
    })
}

fn method_not_found(id: Value, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32601,
            "message": format!("method not found: {method}")
        }
    })
}
