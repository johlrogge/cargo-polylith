use std::fmt;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use super::model::{VersioningPolicy, WorkspaceMap};
use super::{classify_dep, transitive_closure, version, DepKind};

/// A single violation found during `check`.
#[derive(Debug, Clone)]
pub struct Violation {
    pub kind: ViolationKind,
}

impl Serialize for Violation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Violation", 2)?;
        s.serialize_field("kind", &self.kind)?;
        s.serialize_field("message", &self.kind.to_string())?;
        s.end()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// A component is missing its expected lib.rs re-export file.
    MissingLibRs { brick: String },
    /// A component is missing its implementation file (`src/<name>.rs`).
    MissingImplFile { brick: String },
    /// A base is missing its `src/lib.rs` — bases must expose a runtime API as a library.
    BaseMissingLibRs { brick: String },
    /// A base has a `src/main.rs` — executable entry points belong in projects, not bases.
    BaseHasMainRs { brick: String },
    /// A component is not depended on by any base or project (potential dead code).
    OrphanComponent { brick: String },
    /// A component's lib.rs uses a wildcard re-export (`pub use <name>::*`).
    WildcardReExport { brick: String, rust_name: String },
    /// A project has no dependency on any base.
    ProjectMissingBase { project: String },
    /// A component or base exists in its polylith directory but is not listed in the root
    /// workspace members, so `cargo build --workspace` will silently ignore it.
    NotInRootWorkspace { brick: String, kind_label: String, rel: String },
    /// Two or more components declare the same interface name but none has a package name
    /// matching the interface — every consumer must `[patch]` explicitly (no default impl).
    AmbiguousInterface { interface: String, impls: Vec<String> },
    /// Two or more components share the same package name — likely a stub that was named
    /// identically to the real component instead of getting a distinct name.
    DuplicateName { name: String, paths: Vec<String> },
    /// A component has no `interface` declared in `[package.metadata.polylith]`.
    MissingInterface { brick: String },
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
        rel: String,
    },
    /// A project's Cargo.toml has its own `[workspace]` section — it must be a plain
    /// bin crate in the root workspace instead.
    ProjectHasOwnWorkspace { project: String, rel: String },
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
    /// A brick's Cargo.toml does not use `version.workspace = true` in a relaxed-mode workspace.
    BrickNotUsingWorkspaceVersion { brick_name: String },
}

impl fmt::Display for ViolationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViolationKind::MissingLibRs { brick } => {
                write!(f, "component '{brick}': src/lib.rs is missing")
            }
            ViolationKind::MissingImplFile { brick } => {
                write!(f, "component '{brick}': src/{brick}.rs is missing")
            }
            ViolationKind::BaseMissingLibRs { brick } => {
                write!(f, "base '{brick}': src/lib.rs is missing — bases must expose a runtime API as a library function")
            }
            ViolationKind::BaseHasMainRs { brick } => {
                write!(f, "base '{brick}': src/main.rs should be in a project, not a base — bases expose library functions like `run()` that projects call")
            }
            ViolationKind::OrphanComponent { brick } => {
                write!(f, "component '{brick}' is not used by any base, project, or profile")
            }
            ViolationKind::WildcardReExport { brick, rust_name } => {
                write!(f, "component '{brick}': lib.rs uses wildcard re-export — consider explicit `pub use {rust_name}::{{Type, fn}};`")
            }
            ViolationKind::ProjectMissingBase { project } => {
                write!(f, "project '{project}' has no base dependency — polylith projects must include at least one base")
            }
            ViolationKind::NotInRootWorkspace { brick, kind_label, rel } => {
                write!(f, "{kind_label} '{brick}' is not listed in root workspace members — add '{rel}' to [workspace] members in Cargo.toml")
            }
            ViolationKind::AmbiguousInterface { interface, impls } => {
                write!(f, "interface '{interface}' has {} implementations ({}) but none has the default package name — every consumer must explicitly declare which implementation to use (path + package = \"...\")", impls.len(), impls.join(", "))
            }
            ViolationKind::DuplicateName { name, paths } => {
                write!(f, "package name '{name}' is used by {} bricks ({}) — give each a distinct name and declare `[package.metadata.polylith] interface = \"{name}\"` on both", paths.len(), paths.join(", "))
            }
            ViolationKind::MissingInterface { brick } => {
                write!(f, "component '{brick}' has no `[package.metadata.polylith] interface = \"...\"` declaration — add interface metadata or run `cargo polylith edit` to set it")
            }
            ViolationKind::DepKeyMismatch { project, dep_key, expected_name, path } => {
                write!(f, "project '{project}': dep key '{dep_key}' does not match package name '{expected_name}' at {path} — use the correct package name as the dep key, or add `package = \"{expected_name}\"` as an alias")
            }
            ViolationKind::ProjectFeatureDrift { project, dep, project_features, workspace_features } => {
                let proj_set: std::collections::HashSet<_> = project_features.iter().collect();
                let missing: Vec<&String> = workspace_features.iter().filter(|f| !proj_set.contains(f)).collect();
                write!(f, "project '{project}': dep '{dep}' standalone build is missing workspace features {missing:?}")
            }
            ViolationKind::ProjectVersionDrift { project, dep, project_version, workspace_version } => {
                write!(f, "project '{project}': dep '{dep}' version '{project_version}' differs from workspace version '{workspace_version}' — standalone build may resolve a different version")
            }
            ViolationKind::ProjectNotInRootWorkspace { project, rel } => {
                write!(f, "project '{project}' is not listed in root workspace members — add '{rel}' to [workspace] members in Cargo.toml")
            }
            ViolationKind::ProjectHasOwnWorkspace { project, rel } => {
                write!(f, "project '{project}' has its own [workspace] section — remove it from {rel}/Cargo.toml and add the project to the root workspace members")
            }
            ViolationKind::ProfileImplPathNotFound { profile, interface, path } => {
                write!(f, "profile '{profile}': implementation path '{path}' for interface '{interface}' does not exist")
            }
            ViolationKind::ProfileImplNotAComponent { profile, interface, path } => {
                write!(f, "profile '{profile}': '{path}' for interface '{interface}' is not a known workspace component")
            }
            ViolationKind::HardwiredDep { brick, dep } => {
                write!(f, "'{brick}' has a direct path dep on '{dep}' — consider using `{{ workspace = true }}` and the profile wiring diagram")
            }
            ViolationKind::HardwiredImplDep { brick, dep, package } => {
                write!(f, "'{brick}': dep '{dep}' uses `package = \"{package}\"` — this hardwires a specific implementation instead of coding against the interface")
            }
            ViolationKind::BrickNotUsingWorkspaceVersion { brick_name } => {
                write!(f, "brick '{brick_name}' does not use `version.workspace = true` — in relaxed mode all brick versions should follow the workspace version")
            }
        }
    }
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
    let resolve_dep = |dep: &str| -> Vec<String> {
        match classify_dep(dep, map) {
            DepKind::Interface(iface) => map
                .components
                .iter()
                .filter(|c| c.name == iface || c.interface.as_deref() == Some(iface))
                .map(|c| c.name.clone())
                .collect(),
            _ => vec![],
        }
    };

    // Transitive closure: all components reachable from any base or project.
    // Seeds are component *package names* (not interface aliases) so that
    // comp_deps lookups work correctly.
    let comp_deps: std::collections::HashMap<&str, &[String]> = map
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.deps.as_slice()))
        .collect();
    let get_deps = |name: &str| -> Vec<String> {
        comp_deps.get(name).copied().unwrap_or(&[]).to_vec()
    };

    let seeds_from_bricks: Vec<String> = map
        .bases
        .iter()
        .flat_map(|b| b.deps.iter().flat_map(|d| resolve_dep(d)))
        .chain(map.projects.iter().flat_map(|p| {
            p.deps.iter().flat_map(|dep_name| resolve_dep(dep_name))
        }))
        .collect();
    let mut depended_on = transitive_closure(
        seeds_from_bricks,
        get_deps,
        resolve_dep,
    );

    // Seed depended_on from profile implementations: any component selected by a
    // profile is considered used, even if no base or project directly depends on it.
    let profile_seeds: Vec<String> = profiles
        .iter()
        .flat_map(|profile| profile.implementations.values())
        .filter_map(|impl_path| {
            let abs = map.root.join(impl_path);
            let abs_canon = abs.canonicalize().unwrap_or(abs);
            map.components.iter().find(|c| {
                c.path.canonicalize().unwrap_or_else(|_| c.path.clone()) == abs_canon
            })
        })
        .map(|comp| comp.name.clone())
        .collect();
    // Rebuild closures (the originals were moved into the first transitive_closure call
    // above; closures over HashMap do not implement Copy).
    let get_deps = |name: &str| -> Vec<String> {
        comp_deps.get(name).copied().unwrap_or(&[]).to_vec()
    };
    let resolve_dep = |dep: &str| -> Vec<String> {
        match classify_dep(dep, map) {
            DepKind::Interface(iface) => map
                .components
                .iter()
                .filter(|c| c.name == iface || c.interface.as_deref() == Some(iface))
                .map(|c| c.name.clone())
                .collect(),
            _ => vec![],
        }
    };
    // Run BFS again for any components added via profile selections (they may
    // themselves depend on other components).
    let profile_reachable = transitive_closure(
        profile_seeds,
        get_deps,
        resolve_dep,
    );
    depended_on.extend(profile_reachable);

    // --- component checks ---
    for comp in &map.components {
        let lib_rs = comp.path.join("src/lib.rs");
        if !lib_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::MissingLibRs { brick: comp.name.clone() },
            });

            // No lib.rs and no impl file → also flag MissingImplFile
            let impl_file = comp.path.join("src").join(format!("{}.rs", comp.name));
            if !impl_file.exists() {
                violations.push(Violation {
                    kind: ViolationKind::MissingImplFile { brick: comp.name.clone() },
                });
            }
        } else {
            let content = std::fs::read_to_string(&lib_rs).unwrap_or_default();
            // Rust normalises hyphens to underscores in module/crate names.
            let rust_name = comp.name.replace('-', "_");
            let wildcard = format!("pub use {}::*", rust_name);

            if content.contains(&wildcard) {
                violations.push(Violation {
                    kind: ViolationKind::WildcardReExport {
                        brick: comp.name.clone(),
                        rust_name,
                    },
                });
            }
            // If lib.rs exists, any layout (flat, submodule, re-export from deps) is valid.
        }

        if !depended_on.contains(comp.name.as_str()) {
            violations.push(Violation {
                kind: ViolationKind::OrphanComponent { brick: comp.name.clone() },
            });
        }
    }

    // --- project checks ---
    for project in &map.projects {
        let has_base_dep = project.deps.iter().any(|d| base_names.contains(d.as_str()));
        if !has_base_dep {
            violations.push(Violation {
                kind: ViolationKind::ProjectMissingBase { project: project.name.clone() },
            });
        }
    }

    // --- project has own workspace checks ---
    for project in &map.projects {
        if project.has_own_workspace {
            let rel = project
                .path
                .strip_prefix(&map.root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            violations.push(Violation {
                kind: ViolationKind::ProjectHasOwnWorkspace {
                    project: project.name.clone(),
                    rel,
                },
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
                kind: ViolationKind::DuplicateName {
                    name: name.to_string(),
                    paths: paths.iter().map(|s| s.to_string()).collect(),
                },
            });
        }
    }

    // --- missing interface annotation ---
    for comp in &map.components {
        if comp.interface.is_none() {
            violations.push(Violation {
                kind: ViolationKind::MissingInterface { brick: comp.name.clone() },
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
        if impls.len() > 1 && !impls.contains(iface) {
            violations.push(Violation {
                kind: ViolationKind::AmbiguousInterface {
                    interface: iface.to_string(),
                    impls: impls.iter().map(|s| s.to_string()).collect(),
                },
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
                    kind: ViolationKind::NotInRootWorkspace {
                        brick: brick.name.clone(),
                        kind_label: kind_label.to_string(),
                        rel,
                    },
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
                        rel,
                    },
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
                kind: ViolationKind::BaseMissingLibRs { brick: base.name.clone() },
            });
        }

        if main_rs.exists() {
            violations.push(Violation {
                kind: ViolationKind::BaseHasMainRs { brick: base.name.clone() },
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
            });
        }
    }

    // --- relaxed-mode workspace version checks ---
    if let Some(pt) = &map.polylith_toml {
        if pt.versioning_policy == Some(VersioningPolicy::Relaxed) {
            let all_bricks: Vec<_> = map.components.iter().chain(map.bases.iter()).cloned().collect();
            let missing = version::bricks_not_using_workspace_version(&all_bricks);
            for name in missing {
                violations.push(Violation {
                    kind: ViolationKind::BrickNotUsingWorkspaceVersion { brick_name: name },
                });
            }
        }
    }

    violations
}

/// Returns `true` for violation kinds that are warnings (exit 0), `false` for hard errors.
pub fn is_warning_kind(k: &ViolationKind) -> bool {
    match k {
        ViolationKind::OrphanComponent { .. } => true,
        ViolationKind::WildcardReExport { .. } => true,
        ViolationKind::BaseHasMainRs { .. } => true,
        ViolationKind::ProjectMissingBase { .. } => true,
        ViolationKind::NotInRootWorkspace { .. } => true,
        ViolationKind::AmbiguousInterface { .. } => true,
        ViolationKind::DuplicateName { .. } => true,
        ViolationKind::MissingInterface { .. } => true,
        ViolationKind::ProjectFeatureDrift { .. } => true,
        ViolationKind::ProjectVersionDrift { .. } => true,
        ViolationKind::ProjectNotInRootWorkspace { .. } => false,
        ViolationKind::ProjectHasOwnWorkspace { .. } => false,
        ViolationKind::MissingLibRs { .. } => false,
        ViolationKind::MissingImplFile { .. } => false,
        ViolationKind::BaseMissingLibRs { .. } => false,
        ViolationKind::DepKeyMismatch { .. } => false,
        ViolationKind::ProfileImplPathNotFound { .. } => false,
        ViolationKind::ProfileImplNotAComponent { .. } => false,
        ViolationKind::HardwiredDep { .. } => true,
        ViolationKind::HardwiredImplDep { .. } => false,
        ViolationKind::BrickNotUsingWorkspaceVersion { .. } => true,
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
