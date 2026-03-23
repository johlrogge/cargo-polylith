#![allow(unused_imports)]

pub mod check;
pub mod discover;
pub mod model;
pub mod status;

pub use check::{check_profile, run_checks, Violation, ViolationKind};
pub use discover::{build_workspace_map, discover_profiles, find_workspace_root, read_polylith_toml, resolve_profile_workspace, resolve_root};
pub use model::{Brick, BrickKind, ExternalDepInfo, PolylithToml, Profile, Project, ResolvedProfileWorkspace, WorkspaceMap, WorkspacePackageMeta, WorkspacePathDep};
pub use status::{run_status, Divergence, StatusReport};

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
    if let Some(base) = map.bases.iter().find(|b| b.name == dep) {
        return DepKind::Base(base.name.as_str());
    }
    // Priority 2: component package name → resolve to interface
    if let Some(comp) = map.components.iter().find(|c| c.name == dep) {
        let iface = comp.interface.as_deref().unwrap_or(comp.name.as_str());
        return DepKind::Interface(iface);
    }
    // Priority 3: component interface name
    if let Some(comp) = map.components.iter().find(|c| c.interface.as_deref() == Some(dep)) {
        return DepKind::Interface(comp.interface.as_deref().unwrap());
    }
    DepKind::External
}
