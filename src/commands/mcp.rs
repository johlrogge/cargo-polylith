use std::env;
use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::workspace::{build_workspace_map, resolve_root, run_checks, run_status};

pub fn serve(workspace_root: Option<&Path>) -> Result<()> {
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
            "tools/list" => tools_list(id),
            "tools/call" => tools_call(id, &req, &root),
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

fn tools_list(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "polylith_info",
                    "description": "Return workspace info: all components, bases, and projects with their deps",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
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
                },
                {
                    "name": "polylith_check",
                    "description": "Check workspace structure for polylith violations",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "polylith_status",
                    "description": "Show a lenient audit of workspace structure (divergences and suggestions)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                }
            ]
        }
    })
}

fn tools_call(id: Value, req: &Value, root: &Path) -> Value {
    let params = &req["params"];
    let name = params["name"].as_str().unwrap_or("");
    let arguments = &params["arguments"];

    let result_text = match name {
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
