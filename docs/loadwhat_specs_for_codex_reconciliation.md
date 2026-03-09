# Reconciliation: `loadwhat_specs_for_codex.md` vs v1 Spec

## Purpose

This note records the parts of `C:\Users\jonlu\Downloads\loadwhat_specs_for_codex.md` that do not match the authoritative v1 spec in `docs/loadwhat_spec_v1.md`, or no longer match the current repository state.

The authoritative order remains:

1. `docs/loadwhat_spec_v1.md`
2. `AGENTS.md`
3. other planning or worklist documents

## Summary

The downloaded markdown is usable as a worklist, but it is stale in two ways:

- it asks for dynamic regression work that is already present in the repository
- a few expectations in it conflict with the v1 output and exit-code contract

As of this note, the repository already contains dynamic contract coverage in:

- `tests/integration/dynamic_loader_snaps_contract.rs`
- `tests/integration/dynamic_missing_direct.rs`
- `tests/integration/dynamic_loadlibrary_name.rs`
- `tests/integration/dynamic_loadlibrary_fullpath.rs`

## Conflicts with the authoritative v1 spec

### 1. Exit code mismatch

The downloaded markdown says several `run` cases should assert `exit code = 2`.

That is not the public CLI contract in v1.

Per `docs/loadwhat_spec_v1.md`:

- `0` = no issues detected
- `10` = missing/bad image issue detected
- `21` = cannot launch/debug target
- `22` = unsupported architecture

Why `2` appears in some existing tests:

- the harness has a separate `LOADWHAT_TEST_MODE` path
- in test mode, helper wrappers may return `2` for fixture-detection convenience
- that is not the public `loadwhat run ...` exit-code contract

Recommended correction:

- for public-output tests, assert `10` for diagnosed dynamic failures
- use `2` only for harness test-mode wrappers that intentionally exercise `LOADWHAT_TEST_MODE`

### 2. Invalid trace token expectation

The downloaded markdown says trace mode should include:

- `TRACE: loader-snaps`

That conflicts with the v1 spec.

Per `docs/loadwhat_spec_v1.md`:

- trace mode uses the normal token families
- dynamic trace output is:
  - `SEARCH_ORDER` if search context exists
  - `DYNAMIC_MISSING`
  - `SEARCH_PATH` entries if search context exists
- loader-snaps runtime strings appear as `DEBUG_STRING` only in verbose mode
- the spec explicitly says not to introduce a separate loader-snaps token family

Recommended correction:

- replace any `TRACE: loader-snaps` expectation with:
  - `SEARCH_ORDER`
  - `DYNAMIC_MISSING`
  - `SEARCH_PATH`

### 3. Worklist is stale

The downloaded markdown presents most dynamic regression tests as missing work, but the repository already contains them in consolidated form.

Already present:

- dynamic summary mode contract
- dynamic trace mode contract
- dynamic verbose mode contract
- dynamic success with loader-snaps
- dynamic full-path success
- dynamic bad-image classification
- dynamic `OTHER` classification
- dynamic multiple-failure selection
- static precedence over dynamic noise

The only gap that was still materially missing when this note was written was:

- dynamic trace output when search context cannot be constructed

That gap is now covered in:

- `tests/integration/dynamic_loader_snaps_contract.rs`

### 4. "No CLI changes are required" needed clarification

The downloaded markdown says no CLI changes are required.

That is true for public behavior, but one narrow internal hook was still useful to test the "no search context" path deterministically.

What was added:

- no public CLI flags
- no new public token family
- no change to the user-visible contract
- only a scoped test-only env-var hook to force dynamic trace search-context construction failure during integration testing

This is consistent with v1 because it does not alter normal runtime behavior.

## Recommended revision of the downloaded markdown

If `loadwhat_specs_for_codex.md` is kept as a planning note, revise it as follows:

### Replace the "Current state" section

State that the repository already includes the consolidated dynamic contract file:

- `tests/integration/dynamic_loader_snaps_contract.rs`

And state that the remaining worklist is empty or limited to any future cases not already covered there.

### Replace dynamic summary-mode exit-code assertions

Use:

- `exit code = 10` for public CLI regression tests
- `exit code = 2` only for harness test-mode tests, if those are being tested intentionally

### Replace the trace expectation

Use:

- `SEARCH_ORDER`
- `DYNAMIC_MISSING`
- `SEARCH_PATH`

Do not require:

- `TRACE: loader-snaps`

### Mark the "missing without search context" case as implemented

The worklist item for dynamic missing without search context should now be considered implemented.

## Practical guidance

When a planning markdown disagrees with the repo, the safe approach is:

1. follow `docs/loadwhat_spec_v1.md`
2. inspect current tests and code before adding duplicate coverage
3. treat the downloaded file as a stale worklist unless it is updated to match the authoritative spec
