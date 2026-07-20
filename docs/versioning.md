# Contract Versioning & Deprecation (docs/versioning.md)

> Companion to [`PROCESS.md`](../PROCESS.md).
> Source of truth for **how** a downstream consumer reads the contract version,
> which symbols are deprecated today, and how to migrate.
> Required by [issue #151](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/151).

## Where the version lives

Every crate in the workspace ships the same semver string through
`orbitchain_common::version`:

```rust
// orbitchain_common::version
pub const VERSION_STR:   &str = "0.1.0";
pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;
```

- `VERSION_STR` is the single source of truth for the semver string.
- `VERSION_MAJOR` / `VERSION_MINOR` / `VERSION_PATCH` are integer views of
  the same string, useful for Soroban host functions and on-chain comparisons
  where string comparisons would be wasteful.
- The legacy `campaign::VERSION: u32 = 1` constant is **kept for backwards
  compatibility** with pre-0.2 consumers — it is bumped at every minor and
  major release. New consumers should prefer the semver constants above.
- The CLI binary (`crates/tools`) and the JS tutorial (`docs/tutorials/js-quickstart.md`)
  also import `orbitchain_common::version::VERSION_STR` so the same version
  string appears in `cargo run -- version`, `npm run contract:info`, and
  every `cargo doc` run.

## Querying the version on-chain

```bash
# View methods on the campaign contract
stellar contract invoke \
  --id "$CAMPAIGN_CONTRACT_ID" \
  --source $SECRET \
  --network testnet \
  -- version           # → u32, returns campaign::VERSION (legacy int)
stellar contract invoke \
  --id "$CAMPAIGN_CONTRACT_ID" \
  --source $SECRET \
  --network testnet \
  -- version_str       # → String, returns VERSION_STR ("0.1.0")
```

Both methods are stable contract entrypoints and will not be renamed or
removed without a major version bump.

## Currently-deprecated symbols

The CI test `versioning::changelog_lists_all_deprecated_symbols` parses
[`CHANGELOG.md`](../CHANGELOG.md) and asserts it lists every
`#[deprecated(since = ..., note = ...)]` symbol in the workspace. Today:

| Crate             | Symbol                                                                              | Deprecated since | Will be removed in | Replacement                                                                      |
|-------------------|-------------------------------------------------------------------------------------|------------------|---------------------|-----------------------------------------------------------------------------------|
| `orbitchain-campaign` | `CampaignContract::legacy_version_marker`                                          | 0.2.0            | 0.4.0               | `CampaignContract::version_str()` + `orbitchain_common::version::VERSION_STR`    |

To see the live list at any time, run:

```bash
cargo doc -p orbitchain-campaign --target wasm32v1-none --no-deps --open
```

`cargo doc` renders every `#[deprecated]` symbol with a strikethrough and
an inline note linking to the replacement — that visual treatment is the
acceptance criterion for "cargo doc flags `#[deprecated]` warnings as
visible" in [issue #151](https://github.com/OrbitChainLabs/OrbitChain-Contracts/issues/151).

## Adding a new `#[deprecated]` annotation

1. Annotate the symbol:

   ```rust
   #[deprecated(
       since = "0.X.0",
       note = "use REPLACEMENT_SYMBOL instead; will be removed in 0.Y.0"
   )]
   pub fn old_entrypoint(...) { ... }
   ```

2. Ship the replacement under its new name in the same commit.
3. Update [`CHANGELOG.md`](../CHANGELOG.md)'s `## [Unreleased]` section with:
   - `### Deprecated`: list the symbol with its `since` version.
   - `### Removed (planned)`: list the same symbol with its target removal version.
4. Update this file's "Currently-deprecated symbols" table.
5. CI gate `versioning::changelog_lists_all_deprecated_symbols` will fail
   prior to step 3 — let the test prompt you.

Migration overview: see [`PROCESS.md`](../PROCESS.md) § "Deprecation timeline".

## Authorship & change history

| Date         | Author            | Change                                                                                                   |
|--------------|-------------------|----------------------------------------------------------------------------------------------------------|
| 2026-07-20   | MicD746 (#151 PR) | First publication. Workspace version `0.1.0` not yet bumped — this document accompanies the policy-only PR. |
