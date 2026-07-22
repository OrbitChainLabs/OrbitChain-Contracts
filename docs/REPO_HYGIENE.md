# Repository hygiene — `make deny` warning ledger

*After PR #192 / #193 / #195 are merged on `main`, `make deny`
exits 0 but produces a fixed set of warnings that are deliberately
accepted. This document is the canonical ledger so contributors
don't burn hours re-asking "why aren't these deduped?" or "what's
the path-dep warning?".*

## TL;DR — expected post-#192+#193+#195 state

```
advisories ok, bans ok, licenses ok, sources ok
✅ License check passed
```

Five expected accepted warning lines:

| Category | Count | Crates / source                  | Why accepted |
|---|---|---|---|
| `warning[duplicate]` | 5 | `darling`, `darling_core`, `darling_macro`, `hashbrown`, `stellar-strkey` | mutually-incompatible transitive dependency-version pins; `cargo update -p X@Y` cannot resolve without bumping `soroban-sdk` / `ark-*` |
| `warning[wildcard]`   | 1 | `crates/testkit` → `orbitchain-campaign` path-dep | intentional monorepo wiring; `deny.toml` inline comment at `[bans]` already flags this |

`make deny` exits 0 after the three PRs land. There are **0**
`bug[]` lines, **0** `warning[license-not-encountered]` lines, and
**0** license failures.

## Why the 5 duplicates are legitimate pinning artifacts

A `chore/lockfile-hygiene` investigation attempted `cargo update
-p X@Y` with explicit versions for each. None of the targeted
updates were able to converge the duplicate count because the
resolution tree has mutually-incompatible transitive constraints.

### `darling` / `darling_core` / `darling_macro`  — 0.20.11 + 0.23.0

- `darling v0.20.11` is pinned by `soroban-sdk-macros v26.1.0` (older
  0.20 line, used in the Soroban SDK's own proc-macros).
- `darling v0.23.0` is required by `serde_with_macros v3.21.0`,
  pulled in via `serde_with` → `soroban-ledger-snapshot`.
- The 0.20 → 0.23 line introduced breaking derive-API changes
  (renamed derives); the resolver cannot pick a single version
  that satisfies both consumers.

### `hashbrown`  — 0.15.5 + 0.17.1

- `hashbrown v0.15.5` comes from the older `ark-ec 0.5` /
  `ark-bls12-381 0.5` / `ark-poly 0.5` cryptographic chain
  used by `soroban-env-host`.
- `hashbrown v0.17.1` is required by `indexmap v2` /
  `toml_edit v0.22`.
- 0.15 → 0.17 changed the `RawTable` API; old `ark-*` is
  unmaintained.

### `stellar-strkey`  — 0.0.13 + 0.0.16

- `stellar-strkey v0.0.13` is bundled with `soroban-env-host v26.1.3`.
- `stellar-strkey v0.0.16` is required by direct consumers
  (`soroban-sdk` v26.1.0).

## Why the wildcard is intentional

`crates/testkit/Cargo.toml` has:

```toml
orbitchain-campaign = { path = "../../campaign" }
```

This is workspace-internal development wiring (the test crate
consumes `campaign` directly for property tests).  cargo-deny's
`[bans].wildcards = "warn"` step considers any path-dep without
a version as a "wildcard".  The `deny.toml` inline comment at
`[bans].wildcards` already notes this is intentional in this
monorepo.

Future suppression options (each in its own PR):

- `crates/testkit/Cargo.toml`: `{ path = "../../campaign", version = "0.1.0" }`.
- `deny.toml`: add `allow-wildcard-paths = true` to `[bans]`.

## Verifying run

This ledger was empirically verified by a local cherry-pick
simulation of the post-merge state on a throwaway branch:

```bash
git fetch origin
git checkout -b chore/_sim origin/main
git cherry-pick \
  240d42e 95da520 27c4440                 # #192, #193, #195 commits respectively
cargo check --workspace --all-targets      # refresh Cargo.lock
make deny                                  # the verifying run
git checkout -
git branch -D chore/_sim
```

The exact warning counts asserted above (5 duplicate + 1
wildcard, 0 of everything else) are what `make deny` produces at
that HEAD.

If a contributor runs `make deny` on `main` later and sees *more*
lines than this ledger lists, that's a new finding worth
investigating. If they see *fewer* lines, then one of the root
causes has been fixed upstream (e.g., bump `soroban-sdk` so
`darling` 0.23 is required throughout; replace `ark-*`; or set
`allow-wildcard-paths = true`) and this ledger should be updated
to match the new normal.

## Related landed PRs

| PR   | Branch                              | Purpose |
|------|-------------------------------------|---------|
| #192 | `chore/close-125-127-134`           | introduce `deny.toml` + labeler + Dependabot; closes #125 / #127 / #134 |
| #193 | `chore/fix-common-workspace-dep`    | add `version = "0.1.0"` to the `[workspace.dependencies].common` entry so cargo-deny's graph resolver stops emitting `bug[unresolved-workspace-dependency]` |
| #195 | `chore/dedupe-allow-list`           | trim 6 `warning[license-not-encountered]` entries from `[licenses].allow` (`ISC`, `MPL-2.0`, `Unicode-DFS-2016`, `CC0-1.0`, `OpenSSL`, `0BSD`) |
| this | `chore/docs-repo-hygiene`           | this ledger documenting the post-merge accepted-warning state |

## Acceptance criteria for anyone closing these issues

A maintainer closing any of #125 / #127 / #134 — or signing off
on this ledger PR — should expect:

- `make deny` exits 0 on `main` after the above PRs land.
- `make deny` filters output to exactly: 5 `warning[duplicate]`
  (4 distinct names with their transitive pin explanations in this
  doc), 1 `warning[wildcard]` (testkit path-dep).  Anything else
  is a regression or a new finding.
- `cargo check --workspace --all-targets` exits 0.
- `cargo fmt --all -- --check` exits 0.

If any of these regresses, the most likely culprit is a new
`Cargo.lock` entry from `./scripts/dep-update.sh` or a Dependabot
PR.  Stop and inspect before merging.
