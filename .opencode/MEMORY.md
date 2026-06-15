# Vertumnus — Project Memory

> Last updated: 2026-06-15
> Current branch: `main` (actively pushing)
> Milestones M1–M5 ✅ Complete
> All ~214 tests passing (~200 unit + ~14 integration + doc-tests). Four fixture crates produce valid output.
> Clean clippy — zero warnings.
> 🤖 Autonomous mode active — agent works, commits, and pushes without user input.
>     No tags until user reviews at end-of-day.
> 📋 Scaling plan active at `.opencode/plan.md` — Sprint 5 in progress (B3 ✅, next: C3)

## Sprint Progress

### Sprint 1 ✅ Complete
- **A1** — Config-file type mapping registry (`.vertumnus/config.toml`)
- **C1** — Parallel pipeline (`rayon`-based concurrent item mapping/generation)

### Sprint 2 ✅ Complete
- **A2** — `syn` fallback for stable Rust (when nightly rustdoc unavailable)
- **B1** — Auto-detect monomorphization from public API signatures
- **C2** — Incremental cache (IR/annotated IR keyed by source content hash)

### Sprint 3 ✅ Complete
- **A3** — Dependency-aware type resolution (`Cargo.lock` parsing, dep rustdoc peeking)
- **D1** — Community type registry (`vertumnus registry` subcommand, GitHub-hosted mappings)

### Sprint 4 ✅ Complete
- **E1** — Async function support (`pyo3-asyncio` bridge for `async fn`)
- **E2** — Data-carrying enum support (Python classes for enum variants with data)

### Sprint 5 — In Progress
| Item | Status | Notes |
|---|---|---|
| D2 — Batch wrapping | ✅ Complete | `vertumnus batch wrap` subcommand |
| B2 — User monomorphization hints | ✅ Complete | Config-file monomorphization |
| B3 — Generic type parameter erasure | ✅ Complete | PhantomData<T> erased, PyClass generated |
| C3 — Streaming IR for huge crates | ⏭️ Next | Not yet started |
| E3 — Cross-crate workspace wrapping | ❌ Not started | |
| E4 — Plugin system | ❌ Not started | |

## Current Test Counts

| Crate | Tests |
|---|---|
| `vertumnus-builder` | ~10 unit |
| `vertumnus-cli` | ~28 integration |
| `vertumnus-generator` | ~30 unit |
| `vertumnus-inspector` | ~28 unit |
| `vertumnus-mapper` | ~104 unit |
| Doc-tests | ~5 |
| **Total** | **~214** |

## B3 — Generic Type Parameter Erasure Details

**What was built:**
- `generic_params: Vec<String>` added to `StructItem`, `EnumItem`, `FunctionItem` in IR + schema
- Both rustdoc JSON and `syn` parsers extract generic parameter names
- PhantomData fields detected and filtered from Python-visible field mappings
- `are_generics_erasure_safe()` checks if all generic params only appear in PhantomData fields
- Erasure-safe structs get `PyClass` instead of `ManualStub`; non-erased still produce `ManualStub`
- Erased generics filled with `()` in inner type reference (e.g., `_crate::Marker<()>`)
- `PhantomData<T>` type maps to `None`/Native silently in type parser
- Fixture crate `tests/fixtures/phantom-markers/` with 5 test types
- 3 new integration tests: inspect IR, map erasure detection, wrap pipeline
- Closes #XX: generic marker/phantom types now generate Python bindings

## Key Files

```
crates/vertumnus-inspector/src/ir.rs         # IR types with generic_params
crates/vertumnus-inspector/src/inspector.rs  # rustdoc JSON parser (generic param extraction)
crates/vertumnus-inspector/src/syn_parser.rs # syn fallback parser (generic param extraction)
crates/vertumnus-mapper/src/mapper.rs        # map_struct: PhantomData filtering, erasure detection
crates/vertumnus-mapper/src/type_parser.rs   # PhantomData → Native mapping
crates/vertumnus-generator/src/codegen.rs    # is_phantom_data_type, erased inner type refs
schemas/ir.schema.json                       # +generic_params array
tests/fixtures/phantom-markers/              # B3 fixture crate
```

## Known Conventions

### Error Handling
- Library crates use `thiserror` for error types. No `unwrap()` in library code.
- The CLI binary (`vertumnus-cli`) uses `anyhow` for top-level error propagation.
- `Result` return types carry specific error enums, never bare `String` errors.

### Serialization
- `serde` with `#[derive(Serialize, Deserialize)]` on all IR types.
- JSON field naming: `#[serde(rename_all = "snake_case")]`.
- Optional fields: `#[serde(default)]` with `#[serde(skip_serializing_if = "Vec::is_empty")]`.

### Project Layout
- Cargo workspace at root with `resolver = "2"`.
- Each pipeline phase is a separate crate under `crates/`: `vertumnus-{inspector,mapper,generator,builder,cli}`.
- Test fixtures live in `tests/fixtures/` and are excluded from the workspace.

### Testing
- Unit tests live in the same file as the code they test, gated by `#[cfg(test)] mod tests`.
- Run `cargo test` and `cargo check` after every significant change.
- Aim for zero clippy warnings.
- Integration tests run the `vertumnus` binary via `Command`.

### Code Style
- Rust edition 2021.
- Doc comments (`///`) on all public items.
- `clap` derive API for CLI argument parsing.
- Avoid `unsafe` in library and generated code.
- Keep generated code readable and idiomatic.

---

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
