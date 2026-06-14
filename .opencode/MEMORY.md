# Vertumnus — Project Memory

> Last updated: 2026-06-14
> Current branch: `main` (not yet pushed)
> M1 (Inspector+IR) ✅ M2 (Type Mapper) ✅ M3 (Binding Generator) ✅ M4 (Builder+CLI) ✅
> All 93 tests passing. Two fixture crates produce installable wheels.

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


### M3 (Binding Generator) ✅ COMPLETE

```
Branch: main (not yet committed)
```

**What was built:**
- `crates/vertumnus-generator/` — new workspace member
- `generator.rs` — `Generator` struct orchestrating code generation
  - `generate_rust_code()` — produces complete `src/lib.rs` with module-level item definitions + `#[pymodule]` registration
  - `collect_methods_by_type()` — groups `impl` block methods by their parent type
  - `get_crate_doc()` — extracts crate-level doc from first item
  - Skips registering `ManualStub` items (lifetime/generic warnings)
- `codegen.rs` — Rust/PyO3 code generation for each item kind:
  - `generate_function_wrapper()` — `#[pyfunction]` with `PyResult` for fallible, `Option<T>` for nullable, `Ok(...)` wrapper for infallible
  - `generate_struct_wrapper()` — `#[pyclass]` with `inner: _crate::Name` delegation, field getters, method generation. Skips generic parameter fields with `// VERTUMNUS:` comment.
  - `generate_enum_wrapper()` — C-like enums as `#[pyclass] #[derive(Clone)]`, method dispatch via `_crate::Enum::method(self)`. Data-carrying variants get `ManualStub`.
  - `generate_method_wrapper()` — handles `self`/`&self`/`&mut self` receivers, delegates to original impl
  - `generate_trait_stub()` — informational `todo!()` stub
  - `ir_type_to_pyo3_type()` — maps type strings to PyO3 return types (`PyResult<T>` for `Result`, `Option<T>` for `Option`, `Bound<'_, PyAny>` for generics)
  - `is_generic_field()` — detects bare generic param field types
- `stubs.rs` — Python `.pyi` and `__init__.py` generation:
  - `generate_pyi()` — full type stub file with `class`, `def`, `IntEnum` for enums
  - `generate_init_py()` — re-exports from native module
  - `ir_type_to_python_type()` — maps to Python type annotation syntax
  - `partition_map_by_kind()` — separates functions, structs, enums, traits for ordered stub output
  - `is_exportable()` — filters `ManualStub` from `__init__.py` exports (except top-level free functions)
- `lib.rs` — public exports: `Generator`, `GeneratorConfig`, `generate()` convenience fn
- CLI integration: `generate` and `wrap` subcommands invoke generator, write `src/lib.rs`, `<pkg>.pyi`, `python/<pkg>/__init__.py`
- Fixes applied during e2e testing:
  - `MapErr` strategy propagated from return type to function level (mapper fix)
  - Doc comment formatting (space after `///`)
  - Enum method dispatch (`_crate::Enum::method(self)` not `self.inner.method(...)`)
  - `#[pyfunction]`/`#[pyclass]` definitions at **module level**, not inside `#[pymodule]` fn body
  - `ManualStub` items excluded from `m.add_class::<...>()` registration
  - Generic field getters skipped with `// VERTUMNUS:` comment
- 28 unit tests (all passing), e2e test with `simple-math` fixture produces valid output

**Key files:**
```
crates/vertumnus-generator/src/
  generator.rs        # Main Generator struct, orchestration (NEW)
  codegen.rs          # Rust/PyO3 code generation ~1193 lines (NEW)
  stubs.rs            # Python .pyi + __init__.py generation (NEW)
  lib.rs              # Public API exports (NEW)
crates/vertumnus-cli/src/
  main.rs             # Updated: generate + wrap commands call generator
```

**Type coverage in generated code:**
| Rust → PyO3 | Status |
|---|---|---|
| Free functions → `#[pyfunction]` | ✅ |
| Infallible → `Ok(val)` | ✅ |
| `Result<T,E>` → `PyResult<T>` + `.map_err(PyRuntimeError)` | ✅ |
| `Option<T>` → `Option<T>` return | ✅ |
| Struct `Foo` → `#[pyclass]` + `inner: _crate::Foo` + getters | ✅ |
| Struct methods → `#[pymethods]` impl | ✅ |
| C-like enum → `#[pyclass] #[derive(Clone)]` | ✅ |
| Enum methods → `_crate::Enum::method(self)` | ✅ |
| Data-carrying enum → ManualStub with warning | ✅ |
| Lifetime/Generic struct → ManualStub | ✅ |
| Trait → `todo!()` stub | ✅ |

### M4 (Builder + CLI) ✅ COMPLETE

```
Branch: main (not yet committed)
```

**What was built:**
- `crates/vertumnus-builder/` — new workspace member with 8 unit tests
- `builder/lib.rs` — Builder orchestration:
  - `generate_pyproject_toml()` — produces `pyproject.toml` with maturin build config, `module-name = "{pkg}._core"`
  - `generate_cargo_toml()` — produces `Cargo.toml` with pyo3 0.22, path dep to original crate, `[lib] name = "_core"`, `crate-type = ["cdylib"]`
  - `scaffold_all()` — writes both pyproject.toml and Cargo.toml to output directory
  - `run_maturin_build()` — invokes `maturin build --release`, returns path to built `.whl`
  - `run_maturin_develop()` — invokes `maturin develop` for local development
  - `read_crate_name()` — parses crate name from original `Cargo.toml` (preserves hyphens)
- CLI `wrap` command extended to invoke builder after successful generation:
  1. Inspector (rustdoc JSON → IR)
  2. Type Mapper (IR → annotated IR)
  3. Binding Generator (annotated IR → Rust + stubs)
  4. Builder (scaffold pyproject.toml + Cargo.toml → run `maturin build --release`)
- Codegen fixes for M4 compatibility:
  - Added `native_module_name` field to `GeneratorConfig` (default `"_core"`)
  - `#[pymodule]` function name → `fn _core(...)` (not `fn package_name(...)`)
  - `__init__.py` imports from `._core` (not `.package_name`)
  - `ir_type_to_pyo3_type("str")` returns `"str"` (not `"&str"` which caused `&&str`)
  - Field getters clone non-Copy types (String, etc.)
  - Default `derive_debug = false` — guarantees compatibility without Debug bound

**E2E Test Results (both fixtures pass):**
- `simple-math` fixture functions: `add`, `div`, `magnitude`, `factorial_loop`, `safe_div`
- `simple-math` struct: `Point` (x, y, z fields + `distance()` method)
- `simple-math` enum: `Direction` (North, South variants + `offset()` method)
- `string-utils` fixture functions: `reverse`, `word_count`, `is_palindrome`, `truncate`
- `string-utils` struct: `TextProcessor` (prefix, uppercase fields + `process()`, `greet()` methods)
- `string-utils` enum: `ProcessStatus` (Success, Failure variants + `is_ok()` method)
- Errors: `safe_div(1,0)` raises `RuntimeError` ✅
- Warnings: Lifetimes (`&'static str`) → emitted but elided for Python binding ✅
- Total tests: 93 passing (8 builder + 28 generator + 47 mapper + 2 inspector + ...)

**Key files (new/updated):**
```
crates/vertumnus-builder/
  Cargo.toml             # deps: serde, serde_json, thiserror, anyhow
  src/lib.rs             # Builder: scaffold + maturin invocation (440+ lines, 8 tests)
crates/vertumnus-generator/src/
  generator.rs           # GeneratorConfig.native_module_name field added
  codegen.rs             # ir_type_to_pyo3_type fix, is_copy_type, field clone
  stubs.rs               # generate_init_py uses native_module_name
tests/fixtures/string-utils/   # Second test fixture (NEW)
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
