//! Project ontology — the identity a Report tab card groups by.
//!
//! The PRD's data-reality rule: "two cwds inside the same repo (worktrees,
//! subdirs) must collapse to one project." The collapse identity is
//! **(hub, project name)** under `~/Projects/<hub>/<name>`, NOT the raw cwd
//! tail. Outside `~/Projects/` we fall back to `project_tail(cwd)` with no hub.
//!
//! Pure functions, no DB. The grouping pass (report.rs) derives an identity
//! per session cwd; two identities compare equal iff they share a key.

/// Hubs recognized under `~/Projects/<hub>/<name>`. A hub segment that isn't
/// in this set still collapses by its following segment (the project name),
/// keeping the hub in the display prefix but not blocking grouping. Update this
/// list when the directory convention grows.
pub const KNOWN_HUBS: &[&str] = &["NOW", "agents-hq", "personal-hq", "labs", "_archive"];

/// The ontology-derived identity for a cwd. `key` is the grouping token — two
/// cwds with the same key collapse to one project card. `hub` feeds the quiet
/// display prefix (`NOW /`); `name` is the project's display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectIdentity {
    pub hub: Option<String>,
    pub name: String,
    /// Stable grouping key: `"{hub}/{name}"` when a hub is present, else name.
    pub key: String,
}

/// Split an absolute path into its `/`-separated segments, dropping empties.
fn segments(path: &str) -> Vec<String> {
    path.split('/').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
}

/// True if `seg` is a known hub (case-sensitive — the convention is fixed-case).
fn is_known_hub(seg: &str) -> bool {
    KNOWN_HUBS.iter().any(|h| *h == seg)
}

/// The ontology-derived identity for a cwd. Collapse rule: inside
/// `~/Projects/<x>/<name>/...`, `<name>` is the first segment after the hub-
/// bearing position, so any subdir (`.../brain/src`, an in-repo worktree)
/// collapses to the SAME `(hub, name)` as the canonical `.../brain`. A sibling
/// worktree DIRECTORY (`.../brain-feat` next to `.../brain`) is its own name —
/// collapsing those by suffix would merge genuinely distinct projects. Outside
/// `~/Projects/` → `(None, tail)`.
///
/// `home_override` is the HOME directory (not the Projects root) and exists for
/// deterministic unit tests; production callers pass `None` (resolves via
/// `dirs::home_dir()`, matching the override's semantics exactly).
pub fn derive_identity_with(cwd: &str, home_override: Option<&str>) -> ProjectIdentity {
    let home = match home_override.map(std::path::PathBuf::from).or_else(dirs::home_dir) {
        Some(h) => h,
        None => return tail_identity(cwd),
    };
    let home_segs = segments(&home.to_string_lossy());
    let cwd_segs = segments(cwd);

    // cwd must start with $HOME, then "Projects", then at least two more
    // segments (the hub and the project name) to qualify for ontology grouping.
    let projects_idx = home_segs.len(); // index in cwd_segs where "Projects" sits
    if cwd_segs.len() >= projects_idx + 3
        && cwd_segs.get(projects_idx).map(|s| s.as_str()) == Some("Projects")
        && cwd_segs[..projects_idx] == home_segs
    {
        let hub = cwd_segs[projects_idx + 1].clone();
        let name = cwd_segs[projects_idx + 2].clone();
        // A known hub prefixes the key; an unknown hub is still grouped by its
        // following name but kept in the display prefix so the card reads honestly.
        let key = if is_known_hub(&hub) {
            format!("{hub}/{name}")
        } else {
            // Unknown hub: still collapse by name alone so two worktrees of the
            // same repo under an unrecognized hub don't fragment. The hub shows
            // in the prefix but isn't part of the grouping key.
            name.clone()
        };
        return ProjectIdentity {
            hub: Some(hub),
            name,
            key,
        };
    }

    tail_identity(cwd)
}

/// Production entry point — resolves `$HOME` via `dirs`.
pub fn derive_identity(cwd: &str) -> ProjectIdentity {
    derive_identity_with(cwd, None)
}

/// Fallback identity: last path segment, no hub. Mirrors digest's `project_tail`.
fn tail_identity(cwd: &str) -> ProjectIdentity {
    let name = cwd.split('/').next_back().filter(|s| !s.is_empty()).unwrap_or(cwd).to_string();
    let key = name.clone();
    ProjectIdentity { hub: None, name, key }
}

// ─── Tests (pure parts) ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/test";

    #[test]
    fn known_hub_collapses_subdirs_not_siblings() {
        // Canonical and any subdir (incl. in-repo worktrees) collapse to
        // NOW/brain; a SIBLING directory is a distinct project by design —
        // suffix-collapsing would merge genuinely different repos.
        let canon = derive_identity_with("/Users/test/Projects/NOW/brain", Some(HOME));
        let subdir = derive_identity_with("/Users/test/Projects/NOW/brain/src", Some(HOME));
        let deep = derive_identity_with("/Users/test/Projects/NOW/brain/wt/feat", Some(HOME));
        let sibling = derive_identity_with("/Users/test/Projects/NOW/brain-feat", Some(HOME));
        assert_eq!(canon.key, "NOW/brain");
        assert_eq!(canon.hub.as_deref(), Some("NOW"));
        assert_eq!(canon.name, "brain");
        assert_eq!(canon.key, subdir.key, "subdir must collapse");
        assert_eq!(canon.key, deep.key, "nested path must collapse");
        assert_ne!(canon.key, sibling.key, "sibling dir is its own project");
    }

    #[test]
    fn hub_without_name_falls_back_without_panic() {
        // ~/Projects/NOW alone (hub, no project) must not index out of bounds.
        let id = derive_identity_with("/Users/test/Projects/NOW", Some(HOME));
        assert_eq!(id.name, "NOW");
        assert!(id.hub.is_none());
    }

    #[test]
    fn production_home_semantics_match_override() {
        // Regression: the None path must resolve HOME (not HOME/Projects) so
        // real cwds under ~/Projects/<hub>/<name> actually group in production.
        if let Some(home) = dirs::home_dir() {
            let cwd = format!("{}/Projects/NOW/brain", home.to_string_lossy());
            let id = derive_identity_with(&cwd, None);
            assert_eq!(id.key, "NOW/brain");
            assert_eq!(id.hub.as_deref(), Some("NOW"));
        }
    }

    #[test]
    fn different_projects_under_same_hub_do_not_collapse() {
        let a = derive_identity_with("/Users/test/Projects/NOW/brain", Some(HOME));
        let b = derive_identity_with("/Users/test/Projects/NOW/heart", Some(HOME));
        assert_ne!(a.key, b.key);
    }

    #[test]
    fn each_known_hub_groups() {
        for hub in KNOWN_HUBS {
            let id = derive_identity_with(
                &format!("/Users/test/Projects/{hub}/proj"),
                Some(HOME),
            );
            assert_eq!(id.hub.as_deref(), Some(*hub));
            assert_eq!(id.name, "proj");
            assert!(id.key.ends_with("/proj"));
        }
    }

    #[test]
    fn unknown_hub_still_groups_by_name_and_keeps_prefix() {
        // An unrecognized hub segment: subdirs still collapse by the following
        // name, and the hub shows in the display prefix.
        let a = derive_identity_with("/Users/test/Projects/experiments/widget", Some(HOME));
        let sub = derive_identity_with("/Users/test/Projects/experiments/widget/src", Some(HOME));
        assert_eq!(a.key, sub.key, "unknown-hub subdirs must collapse by name");
        assert_eq!(a.hub.as_deref(), Some("experiments"));
        assert_eq!(a.name, "widget");
        // Unknown hub is NOT part of the key.
        assert!(!a.key.contains('/'), "unknown hub excluded from key");
    }

    #[test]
    fn outside_projects_falls_back_to_tail() {
        let id = derive_identity_with("/tmp/some-repo", Some(HOME));
        assert_eq!(id.name, "some-repo");
        assert_eq!(id.key, "some-repo");
        assert!(id.hub.is_none(), "no hub outside ~/Projects");
    }

    #[test]
    fn exactly_at_projects_root_has_no_name_falls_back() {
        // $HOME/Projects alone (no hub/name) → tail fallback, no crash.
        let id = derive_identity_with("/Users/test/Projects", Some(HOME));
        assert_eq!(id.name, "Projects");
        assert!(id.hub.is_none());
    }

    #[test]
    fn home_not_resolvable_degrades_gracefully() {
        // No home override AND dirs::home_dir() in the test env still resolves,
        // so this just confirms the tail path is taken for a bare relative cwd.
        let id = derive_identity_with("bare-relative", None);
        assert_eq!(id.name, "bare-relative");
        assert!(id.hub.is_none());
    }

    #[test]
    fn trailing_slash_and_empty_segments_handled() {
        let id = derive_identity_with("/Users/test/Projects/NOW/brain//", Some(HOME));
        assert_eq!(id.key, "NOW/brain");
    }
}
