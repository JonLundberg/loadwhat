# Clippy Fix Summary

## Command

```powershell
cargo clippy -- -D warnings
```

## Situation

The command failed because Clippy warnings are promoted to errors by `-D warnings`.

This is not a harness problem and not a failing-test problem. The code builds and the test workflows still run, but the repository is not yet Clippy-clean under the current toolchain.

## Reported issues

### 1. `clippy::useless_vec`

Most failures are in `src/main.rs`.

Pattern:

```rust
emit(TOKEN_X, &vec![...]);
```

Clippy expects this to be written as a slice:

```rust
emit(TOKEN_X, &[...]);
```

Why:

- `vec![]` allocates unnecessarily
- the call site only needs a borrowed slice
- this is a mechanical cleanup, not a behavior change

Count observed: 31 instances.

### 2. `clippy::manual_c_str_literals`

One failure is in `src/win.rs`.

Current pattern:

```rust
b"IsWow64Process2\0"
```

Recommended replacement:

```rust
c"IsWow64Process2"
```

Why:

- Clippy prefers native C string literals over manually writing a nul-terminated byte string
- this is also a mechanical modernization change

Count observed: 1 instance.

### 3. `clippy::collapsible_if`

One failure is in `src/main.rs`.

Current shape:

```rust
if condition_a {
    if condition_b {
        ...
    }
}
```

Recommended shape:

```rust
if condition_a && condition_b {
    ...
}
```

Why:

- Clippy wants the nested conditional simplified
- this should not change behavior if rewritten directly

Count observed: 1 instance.

## Risk assessment

The fixes are low risk.

- `useless_vec` changes are allocation cleanups only
- the C string literal change is API-call preparation cleanup
- the collapsed `if` is a formatting/simplification change

None of these should change the public CLI contract when applied correctly.

## Recommended fix order

1. Replace all `&vec![...]` call sites with `&[...]`
2. Replace the manual nul-terminated string in `src/win.rs`
3. Collapse the nested `if` in `src/main.rs`
4. Re-run:

```powershell
cargo clippy -- -D warnings
```

## Expected outcome

After these changes, the repository should pass Clippy for the currently reported issues, assuming no additional warnings appear after the first cleanup pass.
