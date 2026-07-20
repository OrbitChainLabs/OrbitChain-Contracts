# Changelog

All notable changes to this workspace are recorded in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to the rules defined in [`PROCESS.md`](PROCESS.md) and
[`docs/versioning.md`](docs/versioning.md).

The schema of every release section is intentionally machine-checkable:
`orbitchain_common::version::tests::changelog_lists_all_deprecated_symbols`
parses this file and verifies that every `#[deprecated(since = ..., note = ...)]`
symbol in the workspace appears in both the `### Deprecated` and the
`### Removed (planned)` subsections of the `## [Unreleased]` section.
Keep both subsections up to date — failing this test is the canonical way
the release process rejects a missed deprecation.

## [Unreleased]

Tracks the open PR that closes issues #131 (JS dApp tutorial),
[#147](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/147)
(batched multi-campaign donations) and
[#151](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/151)
(versioning + deprecation policy). The next release cut from `main` will
land these as `0.2.0` (a minor bump, see `PROCESS.md` § "Pre-1.0 caveat").

### Added

- `orbitchain-batch-donor` Soroban contract crate under `crates/contracts/batch-donor/`
  exposing two entrypoints:
  - `batch_donate(env, operator, targets)` — atomic multi-campaign donation
    in a single Soroban transaction (issue [#147](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/147)).
  - `batch_donate_continue_on_error(env, operator, targets)` — best-effort
    variant that collects per-target outcomes via `try_invoke_contract`
    instead of panic-reverting.
- New `docs/tutorials/js-quickstart.md` demonstrating end-to-end Soroban
  dApp integration from JavaScript using `@stellar/freighter-kit` and
  `@stellar/soroban-client` (issue [#131](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/131)).
- Workspace-level semver constants in `orbitchain_common::version`:
  `VERSION_STR` (`"0.1.0"`), `VERSION_MAJOR` (`0`), `VERSION_MINOR` (`1`),
  `VERSION_PATCH` (`0`).
- New method `CampaignContract::version_str()` returns the workspace version
  as a `String` for native Soroban callers; legacy `CampaignContract::version()`
  is preserved and continues to return the integer `campaign::VERSION`.
- `PROCESS.md` (project root) documenting the workspace-wide version-bump
  rules and deprecation timeline, and `docs/versioning.md` summarising how
  to read those constants and migrate off deprecated symbols (issue
  [#151](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/151)).

### Deprecated

- `orbitchain-campaign::CampaignContract::legacy_version_marker`
  (deprecated since 0.2.0). Use `CampaignContract::version_str()` or
  `orbitchain_common::version::VERSION_STR` instead. Will be removed in 0.4.0.

### Removed (planned)

- `orbitchain-campaign::CampaignContract::legacy_version_marker`
  is scheduled to be removed in release 0.4.0 (three minor releases
  after the deprecation was introduced, per `PROCESS.md` § "Deprecation timeline").

## [0.1.0] — initial workspace

The initial release of the OrbitChain workspace. The canonical
`orbitchain-campaign` Soroban contract; the deprecated
`orbitchain-core` reference contract; the `orbitchain-token-bridge`
contract; the `orbitchain-common` shared types crate; and the
`orbitchain-tools` CLI binary. The wallet-connect mobile deep-link
flow (Lobstr / Bitnovo, SEP-10 + SEP-0007) is shipped in this
release and is the basis for `docs/tutorials/dapp-integration.md`,
which remains the in-repo guide for wallet connect.

[Unreleased]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/compare/main...HEAD
[0.1.0]: https://github.com/OrbitChainLabs/OrbitChain-Contracts/releases/tag/v0.1.0
