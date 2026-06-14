# Vertumnus — Project Memory

> Last updated: 2026-06-14

## Current Milestone: M1 (Inspector + IR) ✅ COMPLETE

### Status
- [x] Workspace root `Cargo.toml` with 5 crate members
- [x] IR schema (`schemas/ir.schema.json`) — version 0.1
- [x] IR types in `vertumnus-inspector` (Rust structs + serde)
- [x] Rustdoc JSON parser (inspector) handles:
  - Functions (primitives, `Option`, `Result`, generics)
  - Structs (named fields, methods from impl blocks)
  - Enums (plain variants, methods)
  - Traits (methods)
  - Impl blocks (inherent + trait, synthetic filtering)
  - Lifetimes (detected, preserved in type strings)
- [x] CLI `vertumnus inspect <path>` command works end-to-end
- [x] Test fixture `simple-math` with functions, structs, enums, generics, lifetimes
- [x] All 13 tests passing

### Key Decisions Made
| Decision | Choice | Rationale |
|---|---|---|
| API Inspection | rustdoc JSON (nightly) | Per spec recommendation, machine-readable |
| Template engine | Deferred to M3 | Will use minijinja for runtime flexibility |
| Error handling | `thiserror` (lib) + `anyhow` (CLI) | Clean error types + ergonomic binary |
| IR serialization | JSON via serde | As specified in §8 |
| `__repr__`/`__eq__` | Deferred to M3 | Auto-derive if Debug/PartialEq detected |

### Rustdoc JSON Format Learned
The actual rustdoc JSON format (as of Rust 1.95) differs from initial assumptions:
- Items use `inner` key (not `kind`) with keys: `function`, `struct`, `enum`, `module`, `struct_field`, `variant`, `impl`
- Types use single-key discriminator: `{"primitive": "i64"}`, `{"resolved_path": {"path": "Vec", ...}}`
- Function sig has `inputs` as `[[name, type], ...]` tuples
- Struct fields are separate items referenced by ID
- IDs are numeric, stored as strings in index HashMap

### Files Created
```
Cargo.toml                     # Workspace root
schemas/ir.schema.json         # IR JSON Schema
crates/vertumnus-inspector/
  Cargo.toml
  src/lib.rs                   # Public API: inspect_crate(), ir module re-exports
  src/ir.rs                    # IR type definitions (IntermediateRepresentation, etc.)
  src/inspector.rs             # Rustdoc JSON parser
crates/vertumnus-cli/
  Cargo.toml
  src/main.rs                  # CLI binary with subcommands
  src/lib.rs                   # Placeholder
crates/vertumnus-mapper/       # Stub for M2
crates/vertumnus-generator/    # Stub for M3
crates/vertumnus-builder/      # Stub for M4
tests/fixtures/simple-math/    # Test fixture crate
```

## Next Up: M2 (Type Mapper)

### Tasks
- [ ] Map all primitive types (i8-u128, f32-f64, bool, String, &str)
- [ ] Map `Vec<T>`, `HashMap<K,V>`, `Option<T>`, `Result<T,E>`
- [ ] Map structs → `#[pyclass]`, enums → `#[pyclass]`/IntEnum
- [ ] Emit warnings for unsupported types (lifetimes, `dyn Trait`, generics)
- [ ] `vertumnus map` command works end-to-end
- [ ] Annotated IR output format

### Open Questions
- Exception hierarchy: Should we use a custom `VertumnusError` or just `RuntimeError`?
- How to handle `&str` vs `String` mapping (both → `str` in Python)
