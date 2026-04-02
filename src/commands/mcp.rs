use std::env;
use std::io::{self, BufRead, Write};
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::commands::validate::validate_brick_name;
use crate::scaffold;
use crate::workspace::{build_workspace_map, classify_dep, discover_profiles, resolve_root, run_checks, run_status, DepKind};

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
        json!({
            "name": "polylith_profile_list",
            "description": "List all polylith profiles and their implementation selections",
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
                "name": "polylith_profile_new",
                "description": "Create a new empty profile file at profiles/<name>.profile",
                "inputSchema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "Profile name (without .profile extension)" }
                    }
                }
            }),
            json!({
                "name": "polylith_profile_add",
                "description": "Add or update an implementation selection in a profile",
                "inputSchema": {
                    "type": "object",
                    "required": ["profile", "interface", "implementation"],
                    "properties": {
                        "profile": { "type": "string", "description": "Profile name (without .profile extension)" },
                        "interface": { "type": "string", "description": "Interface dep key" },
                        "implementation": { "type": "string", "description": "Path to the implementation component (relative to workspace root)" }
                    }
                }
            }),
            json!({
                "name": "polylith_base_update",
                "description": "Update metadata on an existing base (e.g. set test-base flag)",
                "inputSchema": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "Base name" },
                        "test_base": { "type": "boolean", "description": "Set to true to mark this as a test-base" }
                    }
                }
            }),
            json!({
                "name": "polylith_migrate_package_meta",
                "description": "Migrate [workspace.package] metadata from Polylith.toml to root Cargo.toml [package]",
                "inputSchema": { "type": "object", "properties": {} }
            }),
        ]);
    }

    json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } })
}

fn tools_call(id: Value, req: &Value, root: &Path, write: bool) -> Value {
    let params = &req["params"];
    let name = params["name"].as_str().unwrap_or("");
    let arguments = &params["arguments"];

    // Each arm returns Ok(text) for success or Err(json_rpc_error_value) for errors.
    // Err values are fully-formed JSON-RPC error responses ready to return.
    let result: Result<String, Value> = match name {
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
                Ok(serde_json::to_string_pretty(&out).unwrap_or_else(|e| e.to_string()))
            }
            Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
        },

        "polylith_deps" => {
            let filter = arguments.get("component").and_then(|v| v.as_str());
            match build_workspace_map(root) {
                Ok(map) => {
                    let bases: Vec<_> = map
                        .bases
                        .iter()
                        .filter(|b| filter.map(|f| b.deps.iter().any(|d| d == f)).unwrap_or(true))
                        .map(|b| {
                            let mut base_deps: Vec<&str> = vec![];
                            let mut component_deps: Vec<&str> = vec![];
                            for dep in &b.deps {
                                match classify_dep(dep, &map) {
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
                        .filter(|p| filter.map(|f| p.deps.iter().any(|d| d == f)).unwrap_or(true))
                        .map(|p| {
                            let mut base_deps: Vec<&str> = vec![];
                            let mut component_deps: Vec<&str> = vec![];
                            for dep in &p.deps {
                                match classify_dep(dep, &map) {
                                    DepKind::Base(name)      => base_deps.push(name),
                                    DepKind::Interface(name) => component_deps.push(name),
                                    DepKind::External        => {}
                                }
                            }
                            json!({ "name": p.name, "base_deps": base_deps, "component_deps": component_deps })
                        })
                        .collect();

                    Ok(serde_json::to_string_pretty(
                        &json!({ "bases": bases, "projects": projects }),
                    )
                    .unwrap_or_else(|e| e.to_string()))
                }
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_check" => match build_workspace_map(root) {
            Ok(map) => {
                let profiles = discover_profiles(root).unwrap_or_default();
                let violations = run_checks(&map, &profiles);
                Ok(serde_json::to_string_pretty(&json!({ "violations": violations }))
                    .unwrap_or_else(|e| e.to_string()))
            }
            Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
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
                Ok(serde_json::to_string_pretty(&out).unwrap_or_else(|e| e.to_string()))
            }
            Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
        },

        // ── read profile tool ───────────────────────────────────────────────
        "polylith_profile_list" => match discover_profiles(root) {
            Ok(profiles) => Ok(serde_json::to_string_pretty(&profiles).unwrap_or_else(|e| e.to_string())),
            Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
        },

        // ── write tools ─────────────────────────────────────────────────────
        "polylith_component_new" | "polylith_base_new" | "polylith_project_new"
        | "polylith_component_update"
        | "polylith_profile_new" | "polylith_profile_add" | "polylith_base_update"
        | "polylith_migrate_package_meta"
            if !write =>
        {
            Err(jsonrpc_error(
                id.clone(),
                -32601,
                "write tools disabled — restart the MCP server with --write to enable scaffolding".to_string(),
            ))
        }

        "polylith_component_new" => {
            let comp_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let interface = arguments
                .get("interface")
                .and_then(|v| v.as_str())
                .unwrap_or(comp_name);
            let r: anyhow::Result<()> = (|| {
                validate_brick_name(comp_name)?;
                scaffold::create_component(root, comp_name, interface)?;
                Ok(())
            })();
            match r {
                Ok(()) => Ok(format!("created component '{comp_name}' with interface '{interface}'")),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_base_new" => {
            let base_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let r: anyhow::Result<()> = (|| {
                validate_brick_name(base_name)?;
                scaffold::create_base(root, base_name)?;
                Ok(())
            })();
            match r {
                Ok(()) => Ok(format!("created base '{base_name}'")),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_project_new" => {
            let project_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let r: anyhow::Result<()> = (|| {
                validate_brick_name(project_name)?;
                scaffold::create_project(root, project_name)?;
                Ok(())
            })();
            match r {
                Ok(()) => Ok(format!("created project '{project_name}'")),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_component_update" => {
            let comp_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let interface = match params["arguments"]["interface"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: interface".to_string()),
            };
            match build_workspace_map(root) {
                Ok(map) => {
                    match map.components.iter().find(|c| c.name == comp_name) {
                        Some(comp) => {
                            match scaffold::write_interface_to_toml(&comp.path, interface) {
                                Ok(()) => Ok(format!("updated component '{comp_name}' interface to '{interface}'")),
                                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
                            }
                        }
                        None => Err(jsonrpc_error(id.clone(), -32000, format!("component '{comp_name}' not found in workspace"))),
                    }
                }
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_profile_new" => {
            let profile_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let r: anyhow::Result<()> = (|| {
                validate_brick_name(profile_name)?;
                scaffold::create_profile(root, profile_name)?;
                Ok(())
            })();
            match r {
                Ok(()) => Ok(format!("created profile '{profile_name}'")),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_profile_add" => {
            let profile_name = match params["arguments"]["profile"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: profile".to_string()),
            };
            let interface = match params["arguments"]["interface"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: interface".to_string()),
            };
            let implementation = match params["arguments"]["implementation"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: implementation".to_string()),
            };
            match scaffold::add_profile_impl(root, profile_name, interface, implementation) {
                Ok(()) => Ok(format!("updated profile '{profile_name}': {interface} → {implementation}")),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_base_update" => {
            let base_name = match params["arguments"]["name"].as_str() {
                Some(s) if !s.is_empty() => s,
                _ => return jsonrpc_error(id, -32602, "missing required parameter: name".to_string()),
            };
            let test_base = arguments.get("test_base").and_then(|v| v.as_bool()).unwrap_or(false);
            match build_workspace_map(root) {
                Ok(map) => {
                    match map.bases.iter().find(|b| b.name == base_name) {
                        Some(base) => {
                            match scaffold::write_test_base_to_toml(&base.path, test_base) {
                                Ok(()) => Ok(format!("updated base '{base_name}': test-base = {test_base}")),
                                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
                            }
                        }
                        None => Err(jsonrpc_error(id.clone(), -32000, format!("base '{base_name}' not found in workspace"))),
                    }
                }
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        "polylith_migrate_package_meta" => {
            match scaffold::migrate_package_meta_to_cargo_toml(root) {
                Ok(msg) => Ok(msg),
                Err(e) => Err(jsonrpc_error(id.clone(), -32000, format!("{e:#}"))),
            }
        }

        _ => Err(jsonrpc_error(id.clone(), -32601, format!("unknown tool: {name}"))),
    };

    match result {
        Ok(text) => jsonrpc_success(id, text),
        Err(err_response) => err_response,
    }
}

fn jsonrpc_error(id: Value, code: i32, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn jsonrpc_success(id: Value, text: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": text }]
        }
    })
}

fn method_not_found(id: Value, method: &str) -> Value {
    jsonrpc_error(id, -32601, format!("method not found: {method}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonrpc_error_has_error_field_not_result() {
        let resp = jsonrpc_error(json!(1), -32000, "something went wrong".to_string());
        assert!(resp.get("error").is_some(), "response must have 'error' field");
        assert!(resp.get("result").is_none(), "response must not have 'result' field");
        assert_eq!(resp["error"]["code"], -32000);
        assert_eq!(resp["error"]["message"], "something went wrong");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["jsonrpc"], "2.0");
    }

    #[test]
    fn jsonrpc_error_invalid_params_code() {
        let resp = jsonrpc_error(json!(42), -32602, "invalid params".to_string());
        assert_eq!(resp["error"]["code"], -32602);
    }

    #[test]
    fn jsonrpc_error_method_not_found_code() {
        let resp = jsonrpc_error(json!(null), -32601, "unknown tool: foo".to_string());
        assert_eq!(resp["error"]["code"], -32601);
        assert_eq!(resp["error"]["message"], "unknown tool: foo");
    }

    #[test]
    fn jsonrpc_success_has_result_not_error() {
        let resp = jsonrpc_success(json!(5), "hello".to_string());
        assert!(resp.get("result").is_some(), "response must have 'result' field");
        assert!(resp.get("error").is_none(), "response must not have 'error' field");
        assert_eq!(resp["result"]["content"][0]["text"], "hello");
    }

    #[test]
    fn method_not_found_returns_jsonrpc_error() {
        let resp = method_not_found(json!(3), "unknown_method");
        assert!(resp.get("error").is_some());
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn tools_call_unknown_tool_returns_error_not_success() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "nonexistent_tool",
                "arguments": {}
            }
        });
        let root = std::path::Path::new("/tmp");
        let resp = tools_call(json!(1), &req, root, false);
        assert!(resp.get("error").is_some(), "unknown tool must return JSON-RPC error, not success");
        assert!(resp.get("result").is_none(), "unknown tool must not return success result");
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn tools_call_write_tool_without_write_flag_returns_error() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "polylith_component_new",
                "arguments": { "name": "my_comp" }
            }
        });
        let root = std::path::Path::new("/tmp");
        let resp = tools_call(json!(2), &req, root, false);
        assert!(resp.get("error").is_some(), "write-disabled tool must return JSON-RPC error");
        assert!(resp.get("result").is_none(), "write-disabled tool must not return success result");
    }
}
