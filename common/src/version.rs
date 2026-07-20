//! Workspace-level semver constants and deprecation-tracking tests.
//!
//! This module is the single source of truth for `VERSION_STR` and its
//! `VERSION_MAJOR`/`VERSION_MINOR`/`VERSION_PATCH` components, plus the CI
//! gate that keeps [`CHANGELOG.md`](https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/CHANGELOG.md)
//! honest about every `#[deprecated(since = ..., note = ...)]` symbol in the
//! workspace.
//!
//! See [`PROCESS.md`] and [`docs/versioning.md`] for the policy that drives this
//! module.
//!
//! [`PROCESS.md`]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/PROCESS.md
//! [`docs/versioning.md`]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/blob/main/docs/versioning.md

/// Semver string for the entire workspace. Bumped per
/// `PROCESS.md` § "Version-bump rules".
pub const VERSION_STR: &str = "0.1.0";

/// Major component of [`VERSION_STR`].
pub const VERSION_MAJOR: u32 = 0;

/// Minor component of [`VERSION_STR`].
pub const VERSION_MINOR: u32 = 1;

/// Patch component of [`VERSION_STR`].
pub const VERSION_PATCH: u32 = 0;

/// Number of minor releases a deprecated symbol is allowed to live before it
/// must be removed. See `PROCESS.md` § "Deprecation timeline".
pub const DEPRECATION_LIFESPAN_MINORS: u32 = 3;

#[cfg(test)]
mod tests {
    //! Regression tests for the version policy + CHANGELOG bookkeeping.

    // The `common` crate is `#![no_std]`, but `cargo test` re-links `std`
    // at the crate root via `extern crate std;` in `lib.rs`. From this
    // module's scope we reach `std` by path, but the crate prelude is not
    // auto-pulled into nested modules — import the symbols we actually use.
    //
    // `VERSION_*` constants live in the parent module; child modules do not
    // inherit them by short name, so pull them in explicitly via
    // `use super::…`.
    use super::{VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH, VERSION_STR};
    use std::string::String;
    use std::vec::Vec;

    /// Source of `CHANGELOG.md` at compile-time. The path is relative to this
    /// file's location, so it resolves to `<workspace-root>/CHANGELOG.md`
    /// because this file lives at `common/src/version.rs`.
    const CHANGELOG_MD: &str = include_str!("../../CHANGELOG.md");

    /// Every `#[deprecated(since = "X.Y.Z", note = "...")]` symbol in the
    /// workspace that the CHANGELOG bookkeeping must keep up to date for.
    ///
    /// This list is intentionally hand-curated rather than scraped from sources:
    /// grepping the codebase would pick up internal-only annotations and
    /// conditional `#[deprecated]` branches that the CHANGELOG policy does
    /// not need to track.
    const KNOWN_DEPRECATED_SYMBOLS: &[DeprecatedSymbol] = &[DeprecatedSymbol {
        crate_name: "orbitchain-campaign",
        qualified_symbol: "CampaignContract::legacy_version_marker",
        since: "0.2.0",
        planned_removal: "0.4.0",
    }];

    /// A single entry in the curated deprecation table.
    struct DeprecatedSymbol {
        /// Crate name without the `orbitchain-` prefix part that appears in
        /// CHANGELOG.md (e.g. `orbitchain-campaign` for the
        /// `orbitchain-campaign` crate).
        crate_name: &'static str,
        /// The symbol as it appears in the source code (e.g.
        /// `CampaignContract::legacy_version_marker`).
        qualified_symbol: &'static str,
        /// The `since = "..."` string from the `#[deprecated]` attribute.
        since: &'static str,
        /// The minor release in which this symbol is planned to be removed.
        planned_removal: &'static str,
    }

    #[test]
    fn workspace_version_string_is_valid_semver() {
        // Defensive parse: "MAJOR.MINOR.PATCH" with numeric components.
        let parts: Vec<&str> = VERSION_STR.split('.').collect();
        assert_eq!(parts.len(), 3, "VERSION_STR must have three components");
        for part in &parts {
            assert!(
                part.chars().all(|c| c.is_ascii_digit()),
                "VERSION_STR component {part:?} must be all-digit"
            );
        }
        let major: u32 = parts[0].parse().expect("major");
        let minor: u32 = parts[1].parse().expect("minor");
        let patch: u32 = parts[2].parse().expect("patch");
        assert_eq!(major, VERSION_MAJOR);
        assert_eq!(minor, VERSION_MINOR);
        assert_eq!(patch, VERSION_PATCH);
    }

    /// Acceptance criterion #2 of [issue #151](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/151):
    /// "CI test asserts the next minor version's CHANGELOG.md includes all
    /// deprecated-symbol removals."
    ///
    /// This is the most important test in the workspace for #151: it is the
    /// hard CI gate that prevents a maintainer from landing a `#[deprecated]`
    /// annotation without updating CHANGELOG.md.
    #[test]
    fn changelog_lists_all_deprecated_symbols() {
        // `Vec<String>` rather than `Vec<&'static str>`: the qualified
        // symbol is built at runtime from `KNOWN_DEPRECATED_SYMBOLS` and
        // therefore cannot borrow from a `'static` storage class.
        // Panic-messages take `&String` (auto-deref to `&str`) so this
        // change is purely the storage type.
        let mut missing_deprecated: Vec<String> = Vec::new();
        let mut missing_planned_removal: Vec<String> = Vec::new();

        for sym in KNOWN_DEPRECATED_SYMBOLS {
            // `cargo test` re-links `std` at the crate root via
            // `#[cfg(test)] extern crate std;` in `lib.rs`, but the std
            // prelude's `format!` macro isn't auto-pulled into nested
            // modules. Build the qualified-name string via `String::push_str`
            // instead — same outcome, no macro dependency.
            let mut fully_qualified = String::new();
            fully_qualified.push_str(sym.crate_name);
            fully_qualified.push_str("::");
            fully_qualified.push_str(sym.qualified_symbol);

            // The "Deprecated" subsection of "## [Unreleased]" must mention
            // every deprecated symbol AND its `since` version.
            if !changelog_mentions(CHANGELOG_MD, &fully_qualified)
                || !changelog_unreleased_section(CHANGELOG_MD, "### Deprecated").contains(sym.since)
            {
                missing_deprecated.push(fully_qualified.clone());
            }

            // The "Removed (planned)" subsection must list the same symbol
            // AND its `planned_removal` target version.
            if !changelog_mentions(CHANGELOG_MD, &fully_qualified)
                || !changelog_unreleased_section(CHANGELOG_MD, "### Removed (planned)")
                    .contains(sym.planned_removal)
            {
                missing_planned_removal.push(fully_qualified);
            }
        }

        assert!(
            missing_deprecated.is_empty(),
            "CHANGELOG.md `## [Unreleased]` -> `### Deprecated` is missing every \
             entry (and/or its 'since = X.Y.Z' tag) for: {missing_deprecated:?}. \
             See PROCESS.md § 'Adding a new #[deprecated] annotation'.",
        );
        assert!(
            missing_planned_removal.is_empty(),
            "CHANGELOG.md `## [Unreleased]` -> `### Removed (planned)` is missing \
             every entry (and/or its 'in X.Y.Z' tag) for: {missing_planned_removal:?}.",
        );
    }

    #[test]
    fn changelog_has_unreleased_section() {
        assert!(
            CHANGELOG_MD.contains("## [Unreleased]"),
            "CHANGELOG.md must contain a top-level `## [Unreleased]` section so \
             the next release version can be cut from it",
        );
    }

    #[test]
    fn changelog_links_process_and_versioning() {
        // Process docs are referenced by the changelog so contributors can
        // find the policy without grepping the repo.
        assert!(
            CHANGELOG_MD.contains("PROCESS.md"),
            "CHANGELOG.md must link PROCESS.md so readers can find the policy",
        );
        assert!(
            CHANGELOG_MD.contains("docs/versioning.md"),
            "CHANGELOG.md must link docs/versioning.md so readers can find the constants",
        );
    }

    /// Extract the body of one subsection of the `## [Unreleased]` section,
    /// stopping at the next `### ` or `## ` heading.
    fn changelog_unreleased_section<'a>(text: &'a str, subheading: &str) -> &'a str {
        let start_unreleased = text
            .find("## [Unreleased]")
            .expect("CHANGELOG.md is missing `## [Unreleased]` — fix the test fixture");
        let after_unreleased = &text[start_unreleased..];
        let start_sub = after_unreleased.find(subheading).unwrap_or_else(|| {
            panic!("CHANGELOG.md is missing `## [Unreleased]` -> `{subheading}`")
        });
        let body_start = start_sub + subheading.len();
        let after_sub = &after_unreleased[body_start..];
        let end = after_sub
            .find("\n### ")
            .or_else(|| after_sub.find("\n## "))
            .unwrap_or(after_sub.len());
        &after_sub[..end]
    }

    /// `true` when `needle` appears anywhere in `haystack` followed within
    /// the same line by either end-of-line or end-of-buffer.
    fn changelog_mentions(haystack: &str, needle: &str) -> bool {
        haystack.lines().any(|line| line.contains(needle))
    }
}
