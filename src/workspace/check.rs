use serde::Serialize;

use super::model::WorkspaceMap;
use super::{classify_dep, DepKind};

/// A single violation found during `check`.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub kind: ViolationKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// A component is missing its expected lib.rs re-export file.
    MissingLibRs,
    /// A component is missing its implementation file (`src/<name>.rs`).
    MissingImplFile,
    /// A base is missing its `src/lib.rs` — bases must expose a runtime API as a library.
    BaseMissingLibRs,
    /// A base has a `src/main.rs` — executable entry points belong in projects, not bases.
    BaseHasMainRs,
    /// A component is not depended on by any base or project (potential dead code).
    OrphanComponent,
    /// A component's lib.rs uses a wildcard re-export (`pub use <name>::*`).
    WildcardReExport,
    /// A project has no dependency on any base.
    ProjectMissingBase,
    /// A component or base exists in its polylith directory but is not listed in the root
    /// workspace members, so `cargo build --workspace` will silently ignore it.
    NotInRootWorkspace,
    /// Two or more components declare the same interface name but none has a package name
    /// matching the interface — every consumer must `[patch]` explicitly (no default impl).
    AmbiguousInterface,
    /// Two or more components share the same package name — likely a stub that was named
    /// identically to the real component instead of getting a distinct name.
    DuplicateName,
    /// A component has no `interface` declared in `[package.metadata.polylith]`.
    MissingInterface,
    /// A project's path dependency key doesn't match the target's `package.name`
    /// and no `package = "..."` alias was provided.
    DepKeyMismatch {
        project: String,
        dep_key: String,
        expected_name: String,
        path: String,
    },
    /// A project's external dep declares fewer features than the root workspace dep —
    /// standalone builds may be missing features that the root workspace unifies.
    ProjectFeatureDrift {
        project: String,
        dep: String,
        project_features: Vec<String>,
        workspace_features: Vec<String>,
    },
    /// A project's external dep specifies a different version than the root workspace dep —
    /// standalone builds may resolve a different crate version.
    ProjectVersionDrift {
        project: String,
        dep: String,
        project_version: String,
        workspace_version: String,
    },
    /// A project exists under projects/ but is not listed in root workspace members.
    ProjectNotInRootWorkspace {
        project: String,
    },
    /// A project's Cargo.toml has its own `[workspace]` section — it must be a plain
    /// bin crate in the root workspace instead.
    ProjectHasOwnWorkspace { project: String },
    /// A profile's implementation path does not exist under the workspace root.
    ProfileImplPathNotFound {
        profile: String,
        interface: String,
        path: String,
    },
    /// A profile's implementation path exists but is not a known workspace component.
    ProfileImplNotAComponent {
        profile: String,
        interface: String,
        path: String,
    },
    /// A component or base has a direct `path = "..."` dep to another workspace member
    /// instead of using `{ workspace = true }` — bypasses the profile wiring diagram.
    HardwiredDep {
        brick: String,
        dep: String,
    },
    /// A dep uses `package = "X"` where X differs from the dep key — hardwires a
    /// specific implementation rather than coding against the interface.
    HardwiredImplDep {
        brick: String,
        dep: String,
        package: String,
    },
}

/// Run all structural checks against `map` and return any violations found.
///
/// `profiles` is used to seed the orphan-component check: any component selected
/// by a profile is considered "depended on" and will not be flagged as an orphan.
/// Pass an empty slice if no profiles are available.
pub fn run_checks(map: &WorkspaceMap, profiles: &[super::model::Profile]) -> Vec<Violation> {
    let mut violations = vec![];

    let base_names: std::collections::HashSet<&str> =
        map.bases.iter().map(|b| b.name.as_str()).collect();

    // Resolve a dep key (which may be an interface alias) to the component package
    // name(s) it refers to. Returns an empty vec for bases and external crates.
    let resolve_dep = |dep: &str| -> Vec<&str> {
        match classify_dep(dep, map) {
            DepKind::Interface(iface) => map
                .components
                .iter()
                .filter(|c| c.name == iface || c.interface.as_deref() == Some(iface))
                .map(|c| c.name.as_str())
                .collect(),
            _ => vec![],
        }
    };

    // Transitive closure: all components reachable from any base or project.
    // The queue contains component *package names* (not interface aliases) so that
    // comp_deps lookups work correctly.
    let comp_deps: std::collections::HashMap<&str, &[String]> = map
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.deps.as_slice()))
        .collect();
    let mut depended_on: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<&str> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().flat_map(|d| resolve_dep(d)))
        .chain(map.projects.iter().flat_map(|p| {
            p.deps.iter().flat_map(|dep_name| resolve_dep(dep_name))
        }))
        .collect();
    while let Some(name) = queue.pop_front() {
        if depended_on.insert(name) {
            if let Some(deps) = comp_deps.get(name) {
                for d in *deps {
                    for resolved in resolve_dep(d) {
                        queue.push_back(resolved);
                    }
                }
            }
        }
    }

    // Seed depended_on from profile implementations: any component selected by a
    // profile is considered used, even if no base or project directly depends on it.
    for profile in profiles {
        for impl_path in profile.implementations.values() {
            let abs = map.root.join(impl_path);
            let abs_canon = abs.canonicalize().unwrap_or(abs);
            if let Some(comp) = map.components.iter().find(|c| {
                c.path.canonicalize().unwrap_or_else(|_| c.path.clone()) == abs_canon
            }) {
                queue.push_back(comp.name.as_str());
            }
        }
    }
    // Run BFS again for any components added via profile selections (they may
    // themselves depend on other components).
    while let Some(name) = queue.pop_front() {
        if depended_on.insert(name) {
            if let Some(deps) = comp_deps.get(name) {
                for d in *deps {
                    for resolved in resolve_dep(d) {
                        queue.push_back(resolved);
                    }
                }
            }
        }
    }

    // --- component checks ---
    for comp in &map.components {
        let lib_rs = comp.path.join("src/lib.rs");
        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::MissingLibRs,
                message: format!("component '{}': src/lib.rs is missing", comp.name),
            });

            // No lib.rs and no impl file → also flag MissingImplFile
            let impl_file = comp.path.join("src").join(format!("{}.rs", comp.name));
            if !impl_file.exists() {
                violations.push(Violation {
                    kind: ViolationKind::MissingImplFile,
                    message: format!(
                        "component '{}': src/{}.rs is missing",
                        comp.name, comp.name
                    ),
                });
            }
        } else {
            let content = std::fs::read_to_string(&lib_rs).unwrap_or_default();
            // Rust normalises hyphens to underscores in module/crate names.
            let rust_name = comp.name.replace('-', "_");
            let wildcard = format!("pub use {}::*", rust_name);

            if content.contains(&wildcard) {
                violations.push(Violation {
                    kind: ViolationKind::WildcardReExport,
                    message: format!(
                        "component '{}': lib.rs uses wildcard re-export — consider explicit `pub use {}::{{Type, fn}};`",
                        comp.name, rust_name
                    ),
                });
            }
            // If lib.rs exists, any layout (flat, submodule, re-export from deps) is valid.
        }

        if !depended_on.contains(comp.name.as_str()) {
            violations.push(Violation {
                kind: ViolationKind::OrphanComponent,
                message: format!("component '{}' is not used by any base, project, or profile", comp.name),
            });
        }
    }

    // --- project checks ---
    for project in &map.projects {
        let has_base_dep = project.deps.iter().any(|d| base_names.contains(d.as_str()));
        if !has_base_dep {
            violations.push(Violation {
                kind: ViolationKind::ProjectMissingBase,
                message: format!(
                    "project '{}' has no base dependency — polylith projects must include at least one base",
                    project.name
                ),
            });
        }
    }

    // --- project has own workspace checks ---
    for project in &map.projects {
        if project.has_own_workspace {
            violations.push(Violation {
                kind: ViolationKind::ProjectHasOwnWorkspace { project: project.name.clone() },
                message: format!(
                    "project '{}' has its own [workspace] section — remove it from {}/Cargo.toml \
                     and add the project to the root workspace members",
                    project.name,
                    project.path.strip_prefix(&map.root).unwrap_or(&project.path).display()
                ),
            });
        }
    }

    // --- dep key mismatch checks ---
    // For every path dep in a project that has no package alias, the dep key must
    // match the target crate's package.name exactly.
    for project in &map.projects {
        for (dep_key, dep_path) in &project.dep_paths {
            let cargo_toml = dep_path.join("Cargo.toml");
            if !cargo_toml.exists() {
                continue;
            }
            let pkg_name = match cargo_toml::Manifest::from_path(&cargo_toml) {
                Ok(m) => match m.package.map(|p| p.name) {
                    Some(n) => n,
                    None => continue,
                },
                Err(_) => continue,
            };
            if dep_key != &pkg_name {
                violations.push(Violation {
                    kind: ViolationKind::DepKeyMismatch {
                        project: project.name.clone(),
                        dep_key: dep_key.clone(),
                        expected_name: pkg_name.clone(),
                        path: dep_path.to_string_lossy().into_owned(),
                    },
                    message: format!(
                        "project '{}': dep key '{}' does not match package name '{}' at {} \
                         — use the correct package name as the dep key, or add `package = \"{}\"` as an alias",
                        project.name, dep_key, pkg_name,
                        dep_path.display(),
                        pkg_name,
                    ),
                });
            }
        }
    }

    // --- duplicate name checks ---
    // Two bricks with the same package name means a stub was mis-named. Cargo would
    // reject both in the same workspace; even if only one is currently a member, the
    // duplication signals a configuration error.
    let mut by_name: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for brick in map.components.iter().chain(map.bases.iter()) {
        by_name.entry(brick.name.as_str()).or_default().push(
            brick.path.strip_prefix(&map.root)
                .map(|p| p.to_str().unwrap_or("?"))
                .unwrap_or("?"),
        );
    }
    for (name, paths) in &by_name {
        if paths.len() > 1 {
            violations.push(Violation {
                kind: ViolationKind::DuplicateName,
                message: format!(
                    "package name '{}' is used by {} bricks ({}) — give each a distinct name and declare `[package.metadata.polylith] interface = \"{}\"` on both",
                    name, paths.len(), paths.join(", "), name
                ),
            });
        }
    }

    // --- missing interface annotation ---
    for comp in &map.components {
        if comp.interface.is_none() {
            violations.push(Violation {
                kind: ViolationKind::MissingInterface,
                message: format!(
                    "component '{}' has no `[package.metadata.polylith] interface = \"...\"` \
                     declaration — add interface metadata or run `cargo polylith edit` to set it",
                    comp.name
                ),
            });
        }
    }

    // --- interface checks ---
    // Group components by declared interface name. Warn when multiple components share
    // an interface but none has a package name matching the interface (no default impl).
    let mut by_interface: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for comp in &map.components {
        if let Some(iface) = comp.interface.as_deref() {
            by_interface.entry(iface).or_default().push(comp.name.as_str());
        }
    }
    for (iface, impls) in &by_interface {
        if impls.len() > 1 && !impls.iter().any(|n| *n == *iface) {
            violations.push(Violation {
                kind: ViolationKind::AmbiguousInterface,
                message: format!(
                    "interface '{}' has {} implementations ({}) but none has the default package name — every consumer must explicitly declare which implementation to use (path + package = \"...\")",
                    iface,
                    impls.len(),
                    impls.join(", ")
                ),
            });
        }
    }

    // --- workspace membership checks ---
    if !map.root_members.is_empty() {
        for brick in map.components.iter().chain(map.bases.iter()) {
            let rel = brick
                .path
                .strip_prefix(&map.root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if !map.root_members.iter().any(|m| member_covers(m, &rel)) {
                let kind_label = match brick.kind {
                    super::model::BrickKind::Component => "component",
                    super::model::BrickKind::Base => "base",
                };
                violations.push(Violation {
                    kind: ViolationKind::NotInRootWorkspace,
                    message: format!(
                        "{kind_label} '{}' is not listed in root workspace members \
                         — add '{rel}' to [workspace] members in Cargo.toml",
                        brick.name
                    ),
                });
            }
        }
    }

    // --- project workspace membership checks ---
    if !map.root_members.is_empty() {
        for project in &map.projects {
            let rel = project
                .path
                .strip_prefix(&map.root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if !map.root_members.iter().any(|m| member_covers(m, &rel)) {
                violations.push(Violation {
                    kind: ViolationKind::ProjectNotInRootWorkspace {
                        project: project.name.clone(),
                    },
                    message: format!(
                        "project '{}' is not listed in root workspace members \
                         — add '{rel}' to [workspace] members in Cargo.toml",
                        project.name
                    ),
                });
            }
        }
    }

    // --- base checks ---
    for base in &map.bases {
        let lib_rs  = base.path.join("src/lib.rs");
        let main_rs = base.path.join("src/main.rs");

        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::BaseMissingLibRs,
                message: format!(
                    "base '{}': src/lib.rs is missing — bases must expose a runtime API as a library function",
                    base.name
                ),
            });
        }

        if main_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::BaseHasMainRs,
                message: format!(
                    "base '{}': src/main.rs should be in a project, not a base — bases expose library functions like `run()` that projects call",
                    base.name
                ),
            });
        }

    }

    // --- project standalone dep drift checks (B & C) ---
    for project in &map.projects {
        let mut ext_deps: Vec<_> = project.external_deps.iter().collect();
        ext_deps.sort_by_key(|(k, _)| k.as_str());
        for (dep, proj_info) in ext_deps {
            let Some(ws_info) = map.root_workspace_deps.get(dep) else { continue };

            // Check B: feature drift — workspace has features the project does not
            let proj_set: std::collections::HashSet<_> = proj_info.features.iter().collect();
            let missing: Vec<String> = ws_info.features.iter()
                .filter(|f| !proj_set.contains(f))
                .cloned()
                .collect();
            if !missing.is_empty() {
                violations.push(Violation {
                    kind: ViolationKind::ProjectFeatureDrift {
                        project: project.name.clone(),
                        dep: dep.clone(),
                        project_features: proj_info.features.clone(),
                        workspace_features: ws_info.features.clone(),
                    },
                    message: format!(
                        "project '{}': dep '{}' standalone build is missing workspace features {:?}",
                        project.name, dep, missing,
                    ),
                });
            }

            // Check C: version drift
            if let (Some(pv), Some(wv)) = (&proj_info.version, &ws_info.version) {
                if pv != wv {
                    violations.push(Violation {
                        kind: ViolationKind::ProjectVersionDrift {
                            project: project.name.clone(),
                            dep: dep.clone(),
                            project_version: pv.clone(),
                            workspace_version: wv.clone(),
                        },
                        message: format!(
                            "project '{}': dep '{}' version '{}' differs from workspace version '{}' \
                             — standalone build may resolve a different version",
                            project.name, dep, pv, wv,
                        ),
                    });
                }
            }
        }
    }

    // --- hardwired path dep checks ---
    // Check for components/bases that directly path-dep on other workspace members
    // instead of using { workspace = true }
    let all_brick_names: std::collections::HashSet<&str> = map
        .components
        .iter()
        .chain(map.bases.iter())
        .map(|b| b.name.as_str())
        .collect();
    let all_interface_names: std::collections::HashSet<&str> = map
        .components
        .iter()
        .filter_map(|c| c.interface.as_deref())
        .collect();

    for brick in map.components.iter().chain(map.bases.iter()) {
        for dep_key in &brick.path_dep_keys {
            // Is this dep_key a known component name or interface name?
            if all_brick_names.contains(dep_key.as_str())
                || all_interface_names.contains(dep_key.as_str())
            {
                violations.push(Violation {
                    kind: ViolationKind::HardwiredDep {
                        brick: brick.name.clone(),
                        dep: dep_key.clone(),
                    },
                    message: format!(
                        "'{}' has a direct path dep on '{}' — consider using `{{ workspace = true }}` and the profile wiring diagram",
                        brick.name, dep_key
                    ),
                });
            }
        }
    }

    // --- hardwired impl dep checks (package = "X" where X != dep key) ---
    // Only fire when the package name is a known polylith component or base —
    // external crate aliases (e.g. `spa_sys = { package = "libspa-sys" }`) are fine.
    // Check bricks (components and bases)
    for brick in map.components.iter().chain(map.bases.iter()) {
        for (dep_key, pkg_name) in &brick.hardwired_pkg_deps {
            if !all_brick_names.contains(pkg_name.as_str()) {
                continue; // external crate rename — not a polylith impl dep
            }
            violations.push(Violation {
                kind: ViolationKind::HardwiredImplDep {
                    brick: brick.name.clone(),
                    dep: dep_key.clone(),
                    package: pkg_name.clone(),
                },
                message: format!(
                    "'{}': dep '{}' uses `package = \"{}\"` — this hardwires a specific implementation instead of coding against the interface",
                    brick.name, dep_key, pkg_name
                ),
            });
        }
    }
    // Check projects
    for project in &map.projects {
        for (dep_key, pkg_name) in &project.hardwired_pkg_deps {
            if !all_brick_names.contains(pkg_name.as_str()) {
                continue; // external crate rename — not a polylith impl dep
            }
            violations.push(Violation {
                kind: ViolationKind::HardwiredImplDep {
                    brick: project.name.clone(),
                    dep: dep_key.clone(),
                    package: pkg_name.clone(),
                },
                message: format!(
                    "project '{}': dep '{}' uses `package = \"{}\"` — this hardwires a specific implementation instead of coding against the interface",
                    project.name, dep_key, pkg_name
                ),
            });
        }
    }

    violations
}

/// Returns `true` for violation kinds that are warnings (exit 0), `false` for hard errors.
pub fn is_warning_kind(k: &ViolationKind) -> bool {
    match k {
        ViolationKind::OrphanComponent => true,
        ViolationKind::WildcardReExport => true,
        ViolationKind::BaseHasMainRs => true,
        ViolationKind::ProjectMissingBase => true,
        ViolationKind::NotInRootWorkspace => true,
        ViolationKind::AmbiguousInterface => true,
        ViolationKind::DuplicateName => true,
        ViolationKind::MissingInterface => true,
        ViolationKind::ProjectFeatureDrift { .. } => true,
        ViolationKind::ProjectVersionDrift { .. } => true,
        ViolationKind::ProjectNotInRootWorkspace { .. } => false,
        ViolationKind::ProjectHasOwnWorkspace { .. } => false,
        ViolationKind::MissingLibRs => false,
        ViolationKind::MissingImplFile => false,
        ViolationKind::BaseMissingLibRs => false,
        ViolationKind::DepKeyMismatch { .. } => false,
        ViolationKind::ProfileImplPathNotFound { .. } => false,
        ViolationKind::ProfileImplNotAComponent { .. } => false,
        ViolationKind::HardwiredDep { .. } => true,
        ViolationKind::HardwiredImplDep { .. } => false,
    }
}

/// Validate a profile against the workspace map.
/// Returns violations for any implementation path that doesn't exist or isn't a known component.
pub fn check_profile(profile: &super::model::Profile, map: &WorkspaceMap) -> Vec<Violation> {
    let mut violations = vec![];
    for (interface, impl_path) in &profile.implementations {
        let abs = map.root.join(impl_path);
        if !abs.exists() {
            violations.push(Violation {
                kind: ViolationKind::ProfileImplPathNotFound {
                    profile: profile.name.clone(),
                    interface: interface.clone(),
                    path: impl_path.clone(),
                },
                message: format!(
                    "profile '{}': implementation path '{}' for interface '{}' does not exist",
                    profile.name, impl_path, interface
                ),
            });
            continue;
        }
        // Must resolve to a known component (by path), canonicalizing both sides to handle symlinks.
        let abs_canon = abs.canonicalize().unwrap_or(abs.clone());
        let known = map.components.iter().any(|c| {
            c.path.canonicalize().unwrap_or(c.path.clone()) == abs_canon
        });
        if !known {
            violations.push(Violation {
                kind: ViolationKind::ProfileImplNotAComponent {
                    profile: profile.name.clone(),
                    interface: interface.clone(),
                    path: impl_path.clone(),
                },
                message: format!(
                    "profile '{}': '{}' for interface '{}' is not a known workspace component",
                    profile.name, impl_path, interface
                ),
            });
        }
    }
    violations
}

/// Returns true if `pattern` (a root workspace members entry) covers `rel_path`
/// (a path relative to the workspace root, using `/` separators).
///
/// Handles the two common forms:
/// - `"components/*"` — matches any direct child of `components/`
/// - `"components/foo"` — exact match
fn member_covers(pattern: &str, rel_path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/*") {
        rel_path
            .strip_prefix(&format!("{prefix}/"))
            .map(|rest| !rest.contains('/'))
            .unwrap_or(false)
    } else {
        rel_path == pattern
    }
}
