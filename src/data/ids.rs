//! Stable slug ids for apps and actions, and time-sortable run ids.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Local};

/// Turns a display name into a slug: lowercase ASCII letters and digits joined by `-`.
///
/// Returns `"item"` when nothing usable remains (e.g. a name made only of symbols).
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !slug.is_empty() {
                slug.push('-');
            }
            dash = false;
            slug.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    if slug.is_empty() {
        "item".to_string()
    } else {
        slug
    }
}

/// Returns `slugify(name)`, suffixed with `-2`, `-3`, … until `taken` reports it free.
pub fn unique_slug(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = slugify(name);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .expect("an unused suffix always exists")
}

/// Returns true when `id` is a valid slug (what [`slugify`] produces).
pub fn is_slug(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Builds a run id `yyyyMMdd-HHmmss-<action-id>-<4 hex>` so ids sort by start time.
pub fn run_id(start: DateTime<Local>, action_id: &str) -> String {
    format!(
        "{}-{}-{:04x}",
        start.format("%Y%m%d-%H%M%S"),
        action_id,
        random_u16()
    )
}

fn random_u16() -> u16 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    hasher.finish() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("  Build + Run!! "), "build-run");
        assert_eq!(slugify("***"), "item");
        assert!(is_slug("my-app-2"));
        assert!(!is_slug("My App"));
        assert!(!is_slug("-a"));
    }

    #[test]
    fn unique() {
        let taken = ["my-app", "my-app-2"];
        assert_eq!(unique_slug("My App", |s| taken.contains(&s)), "my-app-3");
        assert_eq!(unique_slug("Other", |s| taken.contains(&s)), "other");
    }

    #[test]
    fn run_id_format() {
        let start = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 10, 24, 10, 15, 30).unwrap();
        let id = run_id(start, "build");
        assert!(id.starts_with("20261024-101530-build-"), "{id}");
        assert_eq!(id.len(), "20261024-101530-build-".len() + 4);
    }
}
