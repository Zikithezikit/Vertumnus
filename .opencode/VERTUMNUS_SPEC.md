# Vertumnus — Project Specification

> **Tagline:** Transform any Rust crate into a Python package — with minimal manual binding work.

---

## 1. Vision

Vertumnus is a generic, reusable framework and toolchain that bridges the gap between the Rust and Python ecosystems. It targets two audiences equally:

- **Rust library authors** who want to ship first-class Python packages without becoming binding experts.
- **Python developers** who want to consume high-performance Rust libraries without learning Rust.

The name *Vertumnus* (Roman god of transformation and seasons) reflects the core metaphor: taking something in one form and transforming it, naturally and completely, into another.

---

## 2. Problem Statement

Today, wrapping a Rust crate for Python requires:

1. Deep knowledge of PyO3, Maturin, and the Python C extension model.
2. Hand-writing `#[pymodule]`, `#[pyclass]`, `#[pyfunction]` annotations for every public symbol.
3. Manually mapping Rust types (enums, structs, `Result`, `Option`, lifetimes) to Python equivalents.
4. Managing build toolchains, `pyproject.toml`, CI wheels, and platform targets.

This is high-friction, error-prone, and not reusable across crates. Vertumnus automates as much of this as possible and provides principled patterns for what cannot be automated.

---

## 3. Goals

### Primary
- Inspect a Rust crate's public API and generate idiomatic Python bindings with minimal human intervention.
- Produce a complete, publishable Python package (wheel + sdist) from a Rust crate.
- Support both automation (zero-config path) and customization (escape hatches for complex cases).

### Secondary
- Be composable: each Vertumnus component (inspector, mapper, generator, builder) is usable independently.
- Be opinionated about defaults, but not about the user's library design.
- Produce bindings that feel Pythonic — not like a thin Rust wrapper.

### Non-Goals (v1)
- Supporting `async` Rust (future milestone).
- Supporting crates with complex lifetimes or `unsafe` public APIs (document limitations, do not silently mis-generate).
- Becoming a general-purpose FFI tool (focus is Rust → Python only).

---

## 4. Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                    vertumnus CLI                    │
│           (entry point: `vertumnus wrap`)           │
└──────────────────────┬──────────────────────────────┘
                       │
         ┌─────────────▼──────────────┐
         │     Inspector (Phase 1)    │
         │  Parses crate public API   │
         │  via `cargo doc` / syn /   │
         │  rust-analyzer JSON output │
         └─────────────┬──────────────┘
                       │  → Intermediate Representation (IR)
         ┌─────────────▼──────────────┐
         │   Type Mapper (Phase 2)    │
         │  Maps Rust types → Python  │
         │  types + generates stubs   │
         └─────────────┬──────────────┘
                       │  → Annotated IR
         ┌─────────────▼──────────────┐
         │  Binding Generator (Phase 3)│
         │  Emits PyO3-annotated Rust  │
         │  glue code + .pyi stubs     │
         └─────────────┬──────────────┘
                       │  → Generated source files
         ┌─────────────▼──────────────┐
         │    Builder (Phase 4)       │
         │  Runs maturin / sets up    │
         │  pyproject.toml, CI config  │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │    Output Package          │
         │  Installable Python wheel  │
         │  + type stubs + metadata   │
         └────────────────────────────┘
```

---

## 5. Components

### 5.1 Inspector

**Responsibility:** Parse a Rust crate and produce a structured Intermediate Representation (IR) of its public API.

**Inputs:** Path to a Rust crate (a directory with `Cargo.toml`).

**Outputs:** An IR document (JSON or internal struct tree) describing:
- Public functions (name, arguments, return type, doc comments)
- Public structs and their fields
- Public enums and their variants
- Public traits (for informational purposes; binding generation is limited)
- `impl` blocks on public types

**Implementation options (to be decided by agent):**
- Parse via `syn` (Rust AST crate) — most precise, requires writing a Rust binary tool.
- Parse via `cargo doc --output-format json` (rustdoc JSON, stable since Rust 1.76) — easiest, no custom parser needed, recommended starting point.
- Parse via `rust-analyzer` LSP JSON — most complete, most complex.

**Recommendation for v1:** Use rustdoc JSON (`cargo +nightly rustdoc -- -Z unstable-options --output-format json`). It is machine-readable and captures all public API surface including doc comments and type signatures.

---

### 5.2 Type Mapper

**Responsibility:** For each type in the IR, decide how it maps to Python and what PyO3 strategy to use.

**Mapping table (baseline):**

| Rust Type | Python Equivalent | PyO3 Strategy |
|---|---|---|
| `i8`–`i64`, `u8`–`u64`, `i128`, `u128` | `int` | Native |
| `f32`, `f64` | `float` | Native |
| `bool` | `bool` | Native |
| `String`, `&str` | `str` | Native |
| `Vec<T>` | `list[T]` | Native |
| `HashMap<K,V>` | `dict[K,V]` | Native |
| `Option<T>` | `T \| None` | Native |
| `Result<T, E>` | Raises `Exception` on `Err` | `map_err` + `?` |
| `struct Foo` | `class Foo` | `#[pyclass]` |
| `enum Foo` | `class Foo` (or `IntEnum`) | `#[pyclass]` or `#[derive(FromPyObject)]` |
| `tuple (A, B)` | `tuple[A, B]` | Native |
| `&[T]` | `list[T]` or `bytes` | Context-dependent |
| Lifetimes | ⚠️ Not supported in v1 | Emit warning, skip |
| Trait objects `dyn Trait` | ⚠️ Limited support | Document per-case |

**Outputs:** Annotated IR — each symbol decorated with its Python type mapping and the PyO3 construct to use.

---

### 5.3 Binding Generator

**Responsibility:** Emit Rust glue code and Python stubs from the annotated IR.

**Outputs:**
- `src/lib.rs` (or a new file `src/python_bindings.rs`) — PyO3-annotated Rust code.
- `<package_name>.pyi` — Python type stub file for IDE support and type checkers.
- `python/<package_name>/__init__.py` — thin Python shim (re-exports, optional Pythonic wrappers).

**Design principles for generated code:**
- Generated code should be readable and idiomatic, not machine-looking.
- Each generated item should include a comment indicating it was auto-generated and from which source symbol.
- The generator must be deterministic: same IR → same output, always.
- Escape hatches: if a type cannot be mapped, emit a `todo!()` stub with a `// VERTUMNUS: manual binding required` comment rather than failing silently.

**Template engine:** Use a Rust templating library (e.g. `askama` or `minijinja`) or a simple string-builder approach — agent's choice based on complexity.

---

### 5.4 Builder

**Responsibility:** Set up and invoke the build system to produce a distributable Python package.

**Tasks:**
- Write or scaffold `pyproject.toml` (with `[build-system]` using `maturin`).
- Write `Cargo.toml` additions: add `pyo3` as a dependency with the `extension-module` feature.
- Invoke `maturin develop` (for local install) or `maturin build --release` (for wheel).
- Optionally scaffold a GitHub Actions CI workflow for cross-platform wheel builds.

**Binding backend:** Default to **Maturin + PyO3**. This is the most mature, actively maintained, and widely adopted Rust→Python pipeline as of 2025. The agent may revisit this decision if there is strong reason to prefer an alternative (e.g. `pyo3-pack`, `cffi`, `cxx`), but should document the tradeoff.

---

### 5.5 CLI (`vertumnus`)

**Responsibility:** Orchestrate all phases and provide a developer-facing interface.

**Primary command:**
```
vertumnus wrap <path-to-crate> [options]
```

**Options (v1):**

| Flag | Description |
|---|---|
| `--out <dir>` | Output directory for generated files (default: `../py-<crate_name>`, outside the crate directory) |
| `--package-name <name>` | Python package name (default: `py-<crate_name>`) |
| `--dry-run` | Inspect and map only; do not write files |
| `--no-build` | Generate files but do not invoke maturin |
| `--verbose` | Print IR and mapping decisions to stdout |
| `--overwrite` | Overwrite existing output files |

**Secondary commands (v1):**
```
vertumnus inspect <path-to-crate>   # Run Phase 1 only, dump IR as JSON
vertumnus map <ir.json>             # Run Phase 2 only, dump annotated IR
vertumnus generate <annotated.json> # Run Phase 3 only, emit Rust + stub files
```

---

## 6. Tech Stack

| Layer | Technology | Rationale |
|---|---|---|
| CLI tool | Rust + `clap` | Dog-food the language; fast binary |
| API inspection | rustdoc JSON (nightly feature) | Machine-readable, no custom parser |
| AST parsing (if needed) | `syn` crate | Industry standard for Rust proc-macro / codegen work |
| Code generation | `askama` or string templates | Simple and readable generated output |
| Binding runtime | `pyo3` | Most mature Rust→Python binding library |
| Build/package | `maturin` | Best-in-class for PyO3 wheel building |
| Type stubs | Hand-templated `.pyi` | Enables mypy / pyright compatibility |
| Testing | `pytest` (Python side), `cargo test` (Rust side) | Standard per ecosystem |
| CI | GitHub Actions + `maturin-action` | Cross-platform wheel matrix (linux, macos, windows) |

---

## 7. Repository Structure

```
vertumnus/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── vertumnus-cli/          # CLI binary (Phase orchestrator)
│   ├── vertumnus-inspector/    # Phase 1: rustdoc JSON parser → IR
│   ├── vertumnus-mapper/       # Phase 2: IR → annotated IR
│   ├── vertumnus-generator/    # Phase 3: annotated IR → Rust + .pyi
│   └── vertumnus-builder/      # Phase 4: maturin invocation + scaffolding
├── schemas/
│   └── ir.schema.json          # JSON Schema for the Intermediate Representation
├── templates/
│   ├── pyproject.toml.j2       # Jinja/askama template
│   ├── lib_rs.j2               # Binding glue template
│   └── pyi.j2                  # Stub file template
├── tests/
│   ├── fixtures/               # Sample Rust crates for integration tests
│   └── integration/            # End-to-end: wrap fixture → import in Python
├── docs/
│   ├── architecture.md
│   ├── type-mapping.md
│   └── limitations.md
├── README.md
└── VERTUMNUS_SPEC.md           # This file
```

---

## 8. Intermediate Representation (IR) Schema

The IR is the contract between the Inspector and all downstream phases. It must be stable and versioned.

```json
{
  "vertumnus_ir_version": "0.1",
  "crate_name": "my_crate",
  "crate_version": "1.2.3",
  "items": [
    {
      "kind": "function",
      "name": "add",
      "doc": "Adds two integers.",
      "inputs": [
        { "name": "a", "type": "i64" },
        { "name": "b", "type": "i64" }
      ],
      "output": { "type": "i64" },
      "is_unsafe": false
    },
    {
      "kind": "struct",
      "name": "Point",
      "doc": "A 2D point.",
      "fields": [
        { "name": "x", "type": "f64", "visibility": "public" },
        { "name": "y", "type": "f64", "visibility": "public" }
      ],
      "methods": []
    },
    {
      "kind": "enum",
      "name": "Direction",
      "doc": "Cardinal directions.",
      "variants": [
        { "name": "North", "fields": [] },
        { "name": "South", "fields": [] }
      ]
    }
  ]
}
```

---

## 9. Milestones

### M1 — Inspector + IR ✅ COMPLETE
- [x] Parse a simple Rust crate via rustdoc JSON.
- [x] Emit a valid IR JSON for functions, structs, and enums.
- [x] `vertumnus inspect` command works end-to-end.

### M2 — Type Mapper ✅ COMPLETE
- [x] Map all primitive types.
- [x] Map `Vec`, `HashMap`, `Option`, `Result`.
- [x] Emit warnings for unsupported types (lifetimes, `dyn Trait`).
- [x] `vertumnus map` command works end-to-end.

### M3 — Binding Generator ✅ COMPLETE
- [x] Generate PyO3 glue for functions, structs, enums.
- [x] Generate `.pyi` stubs.
- [x] Generated code compiles with `cargo check`.

### M4 — Builder + CLI ✅ COMPLETE
- [x] Scaffold `pyproject.toml` and `Cargo.toml` additions.
- [x] Invoke `maturin build` and produce a `.whl`.
- [x] `vertumnus wrap` works end-to-end on at least two fixture crates.

### M5 — Polish ✅ COMPLETE
- [x] CI workflow template generation.
- [x] `--dry-run` and `--verbose` modes.
- [x] Docs: architecture, type-mapping reference, limitations.
- [x] Integration test suite with 3+ real-world crates.

---

## 10. Known Limitations (v1)

These are documented non-goals for the first version. The agent should emit clear errors or `// VERTUMNUS: manual binding required` comments rather than attempting to handle these:

- **Rust lifetimes in public API** — Cannot be safely represented in Python. Vertumnus will skip and warn.
- **`async fn` in public API** — Requires `pyo3-asyncio` or `tokio`; out of scope for v1.
- **`dyn Trait` return types** — Cannot be mapped without runtime type erasure. Warn and skip.
- **Generic functions not monomorphized** — e.g. `fn foo<T: Display>(x: T)`. Must be resolved manually.
- **`unsafe` public functions** — Will be included in IR but flagged; generator emits a stub with a safety comment.
- **Circular type references** — IR will represent them, but generator may need manual intervention.

---

## 11. Open Questions for the Agent

These decisions are deferred and should be made by the implementing agent with documented rationale:

1. **rustdoc JSON vs `syn`**: Start with rustdoc JSON. If it proves insufficient (e.g. for crates that don't compile cleanly), fall back to `syn`-based parsing.
2. **Template engine**: Use `askama` if compile-time templates are preferred; use `minijinja` if runtime flexibility is needed.
3. **IR format**: JSON is specified above. The agent may use MessagePack or a Rust native format for internal pipeline stages if performance warrants it, but JSON must be the external serialization format.
4. **Error handling strategy in generated code**: Default is to map `Result::Err` to a Python `RuntimeError`. The agent should evaluate whether a richer exception hierarchy (custom `VertumnusError` base class) is worth the complexity.
5. **`__repr__` and `__eq__` generation**: For `#[pyclass]` structs, the agent should decide whether to auto-derive these from Debug/PartialEq if available.
