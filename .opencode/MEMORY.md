# Vertumnus — Project Memory

> Last updated: 2026-06-14
> Current branch: `main` (pushed to origin)
> Last commit: `d5a8ac8` — M1: Inspector + IR — initial project scaffold
> M2 completes the Type Mapper phase.

## Milestone Completion Status

### M1 (Inspector + IR) ✅ COMPLETE

```
M1 commit: d5a8ac8
Branch: main (pushed to origin/main)
```

**What was built:**
- Workspace root `Cargo.toml` with 5 crate members
- IR schema (`schemas/ir.schema.json`) — version 0.1
- IR types in `vertumnus-inspector` (Rust structs + serde)
- Rustdoc JSON parser handles: functions, structs, enums, traits, impl blocks, lifetimes
- CLI `vertumnus inspect <path>` command works end-to-end
- Test fixture `simple-math` with functions, structs, enums, generics, lifetimes
- All 13 tests passing

**Key files:**
```
Cargo.toml                     # Workspace root (members + exclude)
schemas/ir.schema.json         # IR JSON Schema v0.1
crates/vertumnus-inspector/
  src/ir.rs                    # IR types: IntermediateRepresentation, IrItem, FunctionItem, etc.
  src/inspector.rs             # Rustdoc JSON parser (~800 lines)
crates/vertumnus-cli/
  src/main.rs                  # CLI: wrap, inspect, map, generate subcommands
tests/fixtures/simple-math/    # Test fixture with all IR features
```

### M2 (Type Mapper) ✅ COMPLETE

```
M2 commit: (in progress, not yet committed)
Branch: main
```

**What was built:**
- Annotated IR types (`annotated_ir.rs`) — `AnnotatedIr`, `AnnotatedItem`, `TypeMapping`, `PyO3Strategy`, `MappingWarning`
- Type string parser (`type_parser.rs`) — recursive descent parser for Rust type strings
  - Handles primitives, `Vec`, `Option`, `Result`, `HashMap`, `HashSet`, `Box`, `Arc`, `Rc`, `Cow`
  - Handles references (`&T`, `&mut T`, `&'a T`, `&'a mut T`)
  - Handles tuples, slices, arrays, fn pointers
  - Handles `dyn Trait`, `impl Trait` — emit warnings and `ManualStub`
  - Handles lifetimes — emit warnings
  - Handles generic parameters — emit warnings
  - Falls back to `PyClass`/`PyEnum` for named types
- Main mapper (`mapper.rs`) — walks IR items and produces annotated IR
  - Functions: maps inputs/outputs, detects unsafe/async/generics
  - Structs: maps fields, detects lifetimes/generics
  - Enums: maps variants, detects C-like vs data enums
  - Traits: informational only, `ManualStub`
  - Impl blocks: maps methods
- CLI integration: `vertumnus map <ir.json>` command works end-to-end
- CLI integration: `wrap` command chains inspector → mapper
- `--verbose` flag on `map` shows per-item warnings
- `--dry-run` on `wrap` outputs annotated IR
- Schema: `schemas/annotated_ir.schema.json` v0.1
- 47 unit tests in the mapper crate (all passing)

**Type mapping coverage:**
| Rust → Python | Status |
|---|---|
| i8-i128, u8-u128 → int | ✅ |
| f32, f64 → float | ✅ |
| bool → bool | ✅ |
| String, &str → str | ✅ |
| Vec<T> → list[T] | ✅ |
| HashMap<K,V> → dict[K,V] | ✅ |
| Option<T> → T \| None | ✅ |
| Result<T,E> → T (MapErr) | ✅ |
| struct Foo → Foo (pyclass) | ✅ |
| enum Foo → Foo (pyenum) | ✅ |
| (A,B) → tuple[A,B] | ✅ |
| &[T] → list[T] | ✅ |
| [T; N] → list[T] | ✅ (warns) |
| fn(...) → Callable | ✅ |
| &T → T (unwrap) | ✅ |
| dyn Trait → Any (stub) | ✅ |
| impl Trait → Any (stub) | ✅ |
| Lifetimes → warn + stub | ✅ |
| Generic params → warn + stub | ✅ |
| Raw pointers → Any (stub) | ✅ |

**Key files (new/updated):**
```
crates/vertumnus-mapper/src/
  annotated_ir.rs        # Annotated IR types (NEW)
  type_parser.rs         # Type string parser (NEW)  
  mapper.rs              # Main mapper logic (NEW)
  lib.rs                 # Updated exports
crates/vertumnus-cli/src/
  main.rs                # Updated: map + wrap commands use mapper
schemas/
  annotated_ir.schema.json  # Annotated IR schema (NEW)
```

---

## Code Conventions

These conventions apply across all Vertumnus crates.

### Error Handling
- Library crates use `thiserror` for error types. No `unwrap()` in library code.
- The CLI binary (`vertumnus-cli`) uses `anyhow` for top-level error propagation.
- `Result` return types carry specific error enums, never bare `String` errors.
- Use `anyhow::bail!` for early returns in CLI; return typed errors in libraries.

### Serialization
- `serde` with `#[derive(Serialize, Deserialize)]` on all IR types.
- JSON field naming: `#[serde(rename_all = "snake_case")]`.
- Optional fields: `#[serde(default)]` with `#[serde(skip_serializing_if = "Option::is_none")]`
  or `Vec::is_empty` to omit empty collections from output.
- Untagged enums via `#[serde(untagged)]` for polymorphic item types (e.g., `IrItem`).

### Project Layout
- Cargo workspace at root with `resolver = "2"`.
- Each pipeline phase is a separate crate under `crates/`: `vertumnus-{inspector,mapper,generator,builder,cli}`.
- Crate names use kebab-case, matching the directory name.
- Workspace-level metadata (`version`, `edition`, `license`) inherited via `version.workspace = true`.
- Test fixtures live in `tests/fixtures/` and are excluded from the workspace.

### Testing
- Unit tests live in the same file as the code they test, gated by `#[cfg(test)] mod tests`.
- Run `cargo test` and `cargo check` after every significant change.
- Aim for zero clippy warnings.
- Test at multiple levels: unit (per-function), integration (per-pipeline-phase), end-to-end (`wrap`).

### Code Style
- Rust edition 2021.
- Doc comments (`///`) on all public items — every struct field, function parameter, and variant.
- `clap` derive API for CLI argument parsing (`#[derive(Parser)]`, `#[derive(Subcommand)]`).
- Avoid `unsafe` in generated and library code. Only inspected crates may contain `unsafe`.
- Keep generated code readable and idiomatic — not machine-looking.
- Generated items include a comment indicating they were auto-generated and from which source symbol.
- The pipeline is deterministic: same IR → same annotated IR → same generated code.
