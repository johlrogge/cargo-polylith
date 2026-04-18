pub mod api_diff;
pub mod check;
pub mod discover;
pub mod error;
pub mod git;
pub mod model;
pub mod status;
pub mod strict_bump;
pub mod version;

#[allow(unused_imports)]
pub use check::{check_profile, run_checks, run_version_checks, VersionEnforcement};
pub use discover::{build_workspace_map, collect_root_interface_deps, discover_profiles, plan_root_demotion, read_polylith_toml, read_polylith_workspace_package, resolve_profile_workspace, resolve_root};
// Re-exported so callers can match on specific workspace errors without
// reaching into the internal `error` module.
#[allow(unused_imports)]
pub use error::WorkspaceError;
#[allow(unused_imports)]
pub use model::{PolylithToml, Profile, ResolvedProfileWorkspace, RootDemotionPlan, VersioningPolicy, WorkspaceMap, WorkspacePackageMeta};
pub use status::run_status;
pub use version::{BumpLevel, compute_bumped_version};

/// BFS transitive closure over a dependency graph.
///
/// - `seeds`: initial component/base names to start from.
/// - `get_deps`: given a component name, returns the list of its dependency keys.
/// - `resolve`: maps a single dependency key to zero or more component names.
///
/// Returns the set of all reachable component names (including seeds).
///
/// The two common usage patterns are:
/// - **Interface-resolving** (used by `check`): `resolve` calls `classify_dep` so that
///   interface aliases are expanded to concrete component package names.
/// - **Identity** (used by `status`): `resolve` returns `vec![dep_key.to_owned()]` because
///   the dep key is already the component name in that context.
pub fn transitive_closure(
    seeds: impl IntoIterator<Item = impl Into<String>>,
    get_deps: impl Fn(&str) -> Vec<String>,
    resolve: impl Fn(&str) -> Vec<String>,
) -> std::collections::HashSet<String> {
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = seeds.into_iter().map(Into::into).collect();
    while let Some(name) = queue.pop_front() {
        if !visited.contains(&name) {
            visited.insert(name.clone());
            for dep_key in get_deps(&name) {
                for resolved in resolve(&dep_key) {
                    if !visited.contains(&resolved) {
                        queue.push_back(resolved);
                    }
                }
            }
        }
    }
    visited
}

#[cfg(test)]
mod tests {
    use super::transitive_closure;
    use std::collections::HashSet;

    /// Single linear chain: A → B → C. Seeds = [A]. Result includes A, B, C.
    #[test]
    fn transitive_closure_linear_chain() {
        let result = transitive_closure(
            ["a"],
            |name| match name {
                "a" => vec!["b".to_owned()],
                "b" => vec!["c".to_owned()],
                _ => vec![],
            },
            |dep_key| vec![dep_key.to_owned()],
        );
        assert_eq!(result, HashSet::from(["a".to_owned(), "b".to_owned(), "c".to_owned()]));
    }

    /// Diamond graph: A → B, A → C, B → D, C → D. Seeds = [A]. All four reachable.
    #[test]
    fn transitive_closure_diamond() {
        let result = transitive_closure(
            ["a"],
            |name| match name {
                "a" => vec!["b".to_owned(), "c".to_owned()],
                "b" => vec!["d".to_owned()],
                "c" => vec!["d".to_owned()],
                _ => vec![],
            },
            |dep_key| vec![dep_key.to_owned()],
        );
        assert_eq!(
            result,
            HashSet::from(["a".to_owned(), "b".to_owned(), "c".to_owned(), "d".to_owned()])
        );
    }

    /// Interface-resolving resolve: seeds are already resolved component names;
    /// the `resolve` closure maps dep keys (which may be interface aliases) to
    /// concrete component names. Here "logging-impl" depends on dep key "logging"
    /// which resolves to "logging-impl-v2".
    #[test]
    fn transitive_closure_with_resolve_alias() {
        // Seeds are already-resolved component names (as in check.rs usage).
        // "logging-impl" has a dep key "logging" which resolves via interface alias
        // to "logging-impl-v2".
        let result = transitive_closure(
            ["logging-impl"],
            |name| match name {
                "logging-impl" => vec!["core".to_owned()],
                "core" => vec!["logging".to_owned()], // dep key is an interface alias
                _ => vec![],
            },
            |dep_key| match dep_key {
                "logging" => vec!["logging-impl-v2".to_owned()],
                other => vec![other.to_owned()],
            },
        );
        assert!(result.contains("logging-impl"), "seed should be present");
        assert!(result.contains("core"), "direct dep of seed should be reachable");
        assert!(result.contains("logging-impl-v2"), "alias-resolved dep should be reachable");
        // "logging" the interface key is NOT in the result (it's a dep key, not a component name)
        assert!(!result.contains("logging"), "interface dep key should not appear in result");
    }

    /// No deps: single seed, nothing reachable beyond it.
    #[test]
    fn transitive_closure_no_deps() {
        let result = transitive_closure(
            ["a"],
            |_| vec![],
            |dep_key| vec![dep_key.to_owned()],
        );
        assert_eq!(result, HashSet::from(["a".to_owned()]));
    }

    /// Cycle: A → B → A. Should terminate and visit both.
    #[test]
    fn transitive_closure_cycle() {
        let result = transitive_closure(
            ["a"],
            |name| match name {
                "a" => vec!["b".to_owned()],
                "b" => vec!["a".to_owned()],
                _ => vec![],
            },
            |dep_key| vec![dep_key.to_owned()],
        );
        assert_eq!(result, HashSet::from(["a".to_owned(), "b".to_owned()]));
    }
}

/// Classification of a dependency key found in a brick's `[dependencies]`.
///
/// Bricks declare deps by interface name (the TOML key), not by the concrete
/// implementation package name. `classify_dep` resolves a raw dep key to its
/// polylith meaning so display and analysis code doesn't duplicate the lookup logic.
#[derive(Debug, PartialEq)]
pub enum DepKind<'a> {
    /// The dep key is a base name.
    Base(&'a str),
    /// The dep key resolves to this interface name.
    Interface(&'a str),
    /// Not a polylith brick — an external crate or unknown name.
    External,
}

/// Classify a single dependency key from a brick's `[dependencies]`.
///
/// Priority:
/// 1. Base name → `DepKind::Base`
/// 2. Component package name → `DepKind::Interface` (using the component's declared interface,
///    falling back to the package name if no interface is declared)
/// 3. Component interface name → `DepKind::Interface`
/// 4. Otherwise → `DepKind::External`
pub fn classify_dep<'a>(dep: &str, map: &'a WorkspaceMap) -> DepKind<'a> {
    // Priority 1: base name
    if let Some(&idx) = map.base_by_name.get(dep) {
        return DepKind::Base(map.bases[idx].name.as_str());
    }
    // Priority 2: component package name → resolve to interface
    if let Some(&idx) = map.component_by_name.get(dep) {
        let comp = &map.components[idx];
        let iface = comp.interface.as_deref().unwrap_or(comp.name.as_str());
        return DepKind::Interface(iface);
    }
    // Priority 3: component interface name
    if let Some(&idx) = map.component_by_interface.get(dep) {
        return DepKind::Interface(map.components[idx].interface.as_deref().unwrap());
    }
    DepKind::External
}
