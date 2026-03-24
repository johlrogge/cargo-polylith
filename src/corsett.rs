//! Corsett — compact, unique, scroll-driven label shortening.
//!
//! Designed to be extracted into a standalone crate. The public API is
//! intentionally minimal and dependency-free.
//!
//! # Concept
//!
//! Given a group of labels and a target height (chars available for vertical
//! text), corsett shortens each label so that **all labels remain mutually
//! unique** while consuming as few rows as possible.
//!
//! Segments (split on `-` or `_`) are compressed from the *least distinctive*
//! end first: leading segments that are *identical across every name in the
//! group* are compressed to their first character, freeing budget for the
//! parts that actually differ.
//!
//! # Shortening progression (group: "mdma-playback", "mdma-audio", …)
//!
//! ```text
//! target 13 → "mdma-playback"   full
//! target  9 → "m-playback…"     common prefix "mdma" → "m", suffix intact+…
//! target  6 → "m-play…"         suffix also truncated
//! target  4 → "m-p…"
//! target  3 → "m-p"             compact minimum (no …)
//! ```

// ─────────────────────────────────────────────────────────────────────────────
// Fold plan
// ─────────────────────────────────────────────────────────────────────────────

/// A single entry in a fold plan — either a real row (by original index)
/// or a placeholder for a run of hidden rows.
#[derive(Debug, Clone, PartialEq)]
pub enum FoldEntry {
    Row(usize),    // index into the original rows slice
    Hidden(usize), // count of consecutive rows hidden at this position
}

/// Compute a minimal fold plan given total row count, a set of "must show" indices,
/// and the available display height.
///
/// - If `total_rows <= available`: returns every row as `FoldEntry::Row` (nothing to hide).
/// - Otherwise: keeps all `must_show` rows visible and collapses consecutive runs of
///   non-must-show rows into `FoldEntry::Hidden(count)` entries, using as few display
///   rows as possible while showing all priority rows.
///
/// `scroll_offset` is the first row index to include (mirrors the grid scroll position).
pub fn fold_to_height(
    total_rows: usize,
    must_show: &std::collections::HashSet<usize>,
    available: usize,
    scroll_offset: usize,
) -> Vec<FoldEntry> {
    let count = total_rows.saturating_sub(scroll_offset);
    if count <= available {
        return (scroll_offset..total_rows).map(FoldEntry::Row).collect();
    }

    // Count chain rows and gaps in the scroll range
    let mut chain_in_range = 0usize;
    let mut non_empty_gaps = 0usize;
    let mut in_gap = false;
    for i in scroll_offset..total_rows {
        if must_show.contains(&i) {
            chain_in_range += 1;
            in_gap = false;
        } else if !in_gap {
            non_empty_gaps += 1;
            in_gap = true;
        }
    }

    // Minimum display rows needed: chain rows + one placeholder per gap
    let min_needed = chain_in_range + non_empty_gaps;
    // Extra budget: rows beyond the minimum we can fill with non-chain rows
    let extra = available.saturating_sub(min_needed);

    // Build the plan: show chain rows always, fill extra budget with non-chain rows
    // (greedy top-to-bottom), collapse the rest into Hidden entries
    let mut plan = Vec::new();
    let mut hidden = 0usize;
    let mut budget = extra;

    for i in scroll_offset..total_rows {
        if must_show.contains(&i) {
            if hidden > 0 {
                plan.push(FoldEntry::Hidden(hidden));
                hidden = 0;
            }
            plan.push(FoldEntry::Row(i));
        } else if budget > 0 {
            // Use budget to show this non-chain row
            if hidden > 0 {
                plan.push(FoldEntry::Hidden(hidden));
                hidden = 0;
            }
            plan.push(FoldEntry::Row(i));
            budget -= 1;
        } else {
            hidden += 1;
        }
    }
    if hidden > 0 {
        plan.push(FoldEntry::Hidden(hidden));
    }
    plan
}

// ─────────────────────────────────────────────────────────────────────────────
// Core single-name helpers (useful standalone)
// ─────────────────────────────────────────────────────────────────────────────

/// Compact form: first character of each segment split on `-` or `_`, joined
/// with `-`.
///
/// ```text
/// "mdma-playback" → "m-p"
/// "some_module"   → "s-m"
/// "simple"        → "s"
/// ```
pub fn compact(name: &str) -> String {
    name.split(|c| c == '-' || c == '_')
        .filter_map(|seg| seg.chars().next())
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("-")
}

/// Shorten a single `name` to at most `max_len` characters (plain prefix).
///
/// Use `fit_group` when shortening a set of names together — it produces
/// better results by compressing common leading segments first.
///
/// - Full name if it fits.
/// - Compact form if `max_len` is at or below the compact length.
/// - Otherwise: longest prefix that fits, followed by a single `…` char.
pub fn shorten(name: &str, max_len: usize) -> String {
    let name_len = name.chars().count();
    if name_len <= max_len {
        return name.to_owned();
    }
    let c = compact(name);
    if max_len <= c.chars().count() {
        return c;
    }
    let prefix: String = name.chars().take(max_len - 1).collect();
    format!("{prefix}…")
}

// ─────────────────────────────────────────────────────────────────────────────
// Group-aware shortening
// ─────────────────────────────────────────────────────────────────────────────

/// Shorten a group of names to at most `target_h` characters each, keeping
/// all shortened forms mutually unique.
///
/// Common leading segments (identical across *all* names) are compressed to
/// their first character, freeing budget for the distinctive suffix. When any
/// name in the group must be shortened, the compact-prefix form is applied
/// consistently to all names.
///
/// The returned strings may be shorter than `target_h` but will never be
/// identical to each other (unless the full names are already identical, which
/// is invalid input).
pub fn fit_group(names: &[&str], target_h: usize) -> Vec<String> {
    if names.is_empty() {
        return vec![];
    }

    let segs: Vec<Vec<&str>> = names.iter().map(|n| split_segs(n)).collect();
    let common = common_prefix_count(&segs);

    // If every name fits at target_h, return full names.
    let any_too_long = names.iter().any(|n| n.chars().count() > target_h);
    if !any_too_long {
        return names.iter().map(|n| n.to_string()).collect();
    }

    // With common == 0 (no shared leading segments), fall back to per-name shorten.
    if common == 0 {
        return names.iter().map(|n| shorten(n, target_h)).collect();
    }

    names
        .iter()
        .zip(segs.iter())
        .map(|(&name, name_segs)| {
            shorten_with_compact_prefix(name, name_segs, common, target_h)
        })
        .collect()
}

/// Minimum height (max chars) at which `fit_group` produces all-unique results.
pub fn min_group_height(names: &[&str]) -> usize {
    if names.len() <= 1 {
        return names
            .first()
            .map(|n| compact(n).chars().count().max(1))
            .unwrap_or(1);
    }

    let compact_max = names.iter().map(|n| compact(n).chars().count()).max().unwrap_or(1);
    let full_max = names.iter().map(|n| n.chars().count()).max().unwrap_or(1);

    for t in compact_max..=full_max {
        let shortened = fit_group(names, t);
        if all_unique(&shortened) {
            return t;
        }
    }
    full_max
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

fn split_segs(name: &str) -> Vec<&str> {
    name.split(|c| c == '-' || c == '_').collect()
}

/// Number of leading segment positions where all names share the same value.
fn common_prefix_count(segs: &[Vec<&str>]) -> usize {
    if segs.is_empty() {
        return 0;
    }
    let min_len = segs.iter().map(|s| s.len()).min().unwrap_or(0);
    (0..min_len)
        .take_while(|&i| {
            let first = segs[0][i];
            segs.iter().all(|s| s[i] == first)
        })
        .count()
}

/// Build the compact representation of the first `count` segments, appending
/// `…` after each segment that was actually truncated (i.e. had >1 char).
///
/// ```text
/// ["mdma"]          → "m…"
/// ["mdma", "extra"] → "m…-e…"
/// ["m"]             → "m"     (already 1 char — no truncation indicator)
/// ```
fn build_compact_prefix(segs: &[&str], count: usize) -> String {
    segs[..count]
        .iter()
        .map(|s| {
            let mut chars = s.chars();
            let first = chars.next().unwrap_or('?');
            if chars.next().is_some() {
                // segment has >1 char — it was truncated
                format!("{first}…")
            } else {
                first.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// Shorten one name using a compact-prefix strategy.
///
/// Common leading segments are collapsed to `first_char…` (or just
/// `first_char` when the segment is already 1 char); the remaining
/// ("distinctive") segments get as much of the remaining budget as possible,
/// also with a trailing `…` if truncated.
///
/// Each `…` marks exactly one truncation point, so `"m…-au…"` clearly shows
/// both "mdma" and "audio" were cut — unlike `"m-audio…"` which misleadingly
/// implies only the suffix was trimmed.
fn shorten_with_compact_prefix(
    name: &str,
    name_segs: &[&str],
    common: usize,
    target_h: usize,
) -> String {
    let c = compact(name);

    if target_h <= c.chars().count() {
        return c;
    }

    // Compact prefix with per-segment "…" markers.
    let compact_prefix = build_compact_prefix(name_segs, common);
    let cp_len = compact_prefix.chars().count();

    // Distinctive suffix: everything after the common segments, joined by "-".
    let suffix: String = name_segs[common..].join("-");
    let suffix_len = suffix.chars().count();

    if suffix.is_empty() {
        // All segments are common; nothing distinctive to show.
        return c;
    }

    // Layout: <compact_prefix> "-" <suffix_display>
    // where suffix_display is either the full suffix or suffix[:n]+"…"
    let prefix_cost = cp_len + 1; // cp + one "-" separator

    if target_h <= prefix_cost {
        // No room for suffix at all — just the compact prefix with its markers.
        return compact_prefix;
    }

    let remaining = target_h - prefix_cost;

    if suffix_len <= remaining {
        // Suffix fits in full — no trailing "…" needed; the prefix markers
        // already signal that truncation occurred there.
        format!("{compact_prefix}-{suffix}")
    } else {
        // Suffix must also be truncated — reserve 1 char for its "…".
        let suffix_chars = remaining.saturating_sub(1);
        if suffix_chars == 0 {
            format!("{compact_prefix}-…")
        } else {
            let trunc: String = suffix.chars().take(suffix_chars).collect();
            format!("{compact_prefix}-{trunc}…")
        }
    }
}

fn all_unique(items: &[String]) -> bool {
    let set: std::collections::HashSet<&String> = items.iter().collect();
    set.len() == items.len()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── compact ──────────────────────────────────────────────────────────────

    #[test]
    fn compact_hyphen() {
        assert_eq!(compact("mdma-playback"), "m-p");
    }

    #[test]
    fn compact_underscore() {
        assert_eq!(compact("some_module"), "s-m");
    }

    #[test]
    fn compact_single_word() {
        assert_eq!(compact("logger"), "l");
    }

    // ── shorten (single-name, plain prefix) ──────────────────────────────────

    #[test]
    fn shorten_full_when_fits() {
        assert_eq!(shorten("mdma-playback", 20), "mdma-playback");
        assert_eq!(shorten("mdma-playback", 13), "mdma-playback");
    }

    #[test]
    fn shorten_compact_at_minimum() {
        assert_eq!(shorten("mdma-playback", 3), "m-p");
        assert_eq!(shorten("mdma-playback", 1), "m-p");
    }

    #[test]
    fn shorten_ellipsis_intermediate() {
        assert_eq!(shorten("mdma-playback", 8), "mdma-pl…");
    }

    // ── fit_group ────────────────────────────────────────────────────────────

    #[test]
    fn fit_group_full_when_all_fit() {
        let names = &["mdma-playback", "mdma-audio"];
        let result = fit_group(names, 20);
        assert_eq!(result, vec!["mdma-playback", "mdma-audio"]);
    }

    #[test]
    fn fit_group_compresses_common_prefix() {
        // "mdma" (4 chars) is common → "m…" (2 chars); "playback"/"audio" distinctive.
        let names = &["mdma-playback", "mdma-audio"];
        let result = fit_group(names, 9);
        // Layout: "m…" (2) + "-" (1) + suffix + optional "…"
        // prefix_cost = 3; remaining = 6
        // "playback" (8) > 6 → suffix_chars=5, "playba…" → "m…-playba…" (10)? No:
        //   remaining=6, suffix_chars=5, trunc="playb" → "m…-playb…" (9) ✓
        // "audio" (5) ≤ 6 → full suffix (no trailing …) → "m…-audio" (8) ✓
        assert_eq!(result[0], "m…-playb…");
        assert_eq!(result[1], "m…-audio");
    }

    #[test]
    fn fit_group_unique_at_minimum_height() {
        let names = &["mdma-audio", "mdma-acid", "mdma-playback", "mdma-synth"];
        let h = min_group_height(names);
        let shortened = fit_group(names, h);
        assert!(all_unique(&shortened), "not unique at height {h}: {shortened:?}");
    }

    #[test]
    fn fit_group_no_common_prefix_falls_back() {
        let names = &["alpha-foo", "beta-bar"];
        // No common prefix → plain shorten
        let result = fit_group(names, 6);
        assert_eq!(result[0], shorten("alpha-foo", 6));
        assert_eq!(result[1], shorten("beta-bar", 6));
    }

    // ── min_group_height ─────────────────────────────────────────────────────

    #[test]
    fn min_height_single_name() {
        assert_eq!(min_group_height(&["mdma-playback"]), 3); // compact "m-p"
    }

    #[test]
    fn min_height_suffix_collision_resolved() {
        // "mdma-audio" and "mdma-acid" both compact to "m-a" → collision at floor.
        // With "m…" prefix (2 chars) + "-" (1) + 2 suffix chars + "…" (1) = 6 total.
        // At h=6: "m…-au…" vs "m…-ac…" → unique.
        let names = &["mdma-audio", "mdma-acid"];
        let h = min_group_height(names);
        let shortened = fit_group(names, h);
        assert!(all_unique(&shortened), "not unique at height {h}: {shortened:?}");
        assert_eq!(h, 6, "expected min height 6, got {h}");
        assert!(shortened[0].starts_with("m…-"), "{}", shortened[0]);
        assert!(shortened[1].starts_with("m…-"), "{}", shortened[1]);
    }

    // ── fold_to_height ────────────────────────────────────────────────────────

    #[test]
    fn fold_to_height_no_hiding_when_fits() {
        // 5 rows, 10 available → everything fits, return all as Row
        let must_show: std::collections::HashSet<usize> = [1, 3].iter().copied().collect();
        let plan = fold_to_height(5, &must_show, 10, 0);
        assert_eq!(
            plan,
            vec![
                FoldEntry::Row(0),
                FoldEntry::Row(1),
                FoldEntry::Row(2),
                FoldEntry::Row(3),
                FoldEntry::Row(4),
            ]
        );
    }

    #[test]
    fn fold_to_height_hides_non_chain() {
        // 10 rows, only rows 2 and 5 must show, available = 5 (not enough for all 10)
        let must_show: std::collections::HashSet<usize> = [2, 5].iter().copied().collect();
        let plan = fold_to_height(10, &must_show, 5, 0);
        // chain=2, gaps=3 (rows 0-1, rows 3-4, rows 6-9), min_needed=5, extra=0
        // All non-chain rows get hidden
        // Expect: Hidden(2), Row(2), Hidden(2), Row(5), Hidden(4)
        assert_eq!(
            plan,
            vec![
                FoldEntry::Hidden(2),
                FoldEntry::Row(2),
                FoldEntry::Hidden(2),
                FoldEntry::Row(5),
                FoldEntry::Hidden(4),
            ]
        );
    }

    #[test]
    fn fold_to_height_keeps_rows_when_budget_allows() {
        // 10 rows, must_show=[4], available=8
        // chain=1, gaps=2 (rows 0-3, rows 5-9), min_needed=3, extra=5
        // Budget=5: show rows 0,1,2,3 (4 non-chain, budget→1), chain row 4,
        //           then row 5 (budget→0), then rows 6-9 hidden
        // Plan: Row(0), Row(1), Row(2), Row(3), Row(4), Row(5), Hidden(4)
        let must_show: std::collections::HashSet<usize> = [4].iter().copied().collect();
        let plan = fold_to_height(10, &must_show, 8, 0);
        assert_eq!(
            plan,
            vec![
                FoldEntry::Row(0),
                FoldEntry::Row(1),
                FoldEntry::Row(2),
                FoldEntry::Row(3),
                FoldEntry::Row(4),
                FoldEntry::Row(5),
                FoldEntry::Hidden(4),
            ]
        );
    }
}

#[cfg(test)]
mod mdma_scenario {
    use super::*;

    const MDMA_NAMES: &[&str] = &[
        "mdma-acid", "mdma-audio", "mdma-effects",
        "mdma-mixer", "mdma-playback", "mdma-synth",
    ];

    #[test]
    fn show_scroll_progression() {
        let full_h = MDMA_NAMES.iter().map(|n| n.chars().count()).max().unwrap();
        let min_h  = min_group_height(MDMA_NAMES);
        println!("full_header_h={full_h}  min_unique_h={min_h}");
        for scroll in 0..=(full_h - min_h + 2) {
            let target = full_h.saturating_sub(scroll).max(min_h);
            let names = fit_group(MDMA_NAMES, target);
            let actual_h = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
            println!("scroll={scroll:2}  target={target:2}  actual_h={actual_h:2}  {:?}", names);
        }
    }
}
