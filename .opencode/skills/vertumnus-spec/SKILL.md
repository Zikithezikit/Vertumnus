---
name: vertumnus-spec
description: >-
  Use when implementing or modifying Vertumnus crate components — inspector,
  mapper, generator, builder, or CLI. Provides the IR schema, type mapping
  table, and architecture rules from the project spec.
---

# Vertumnus Specification

This skill encodes the key reference tables and architecture rules from `VERTUMNUS_SPEC.md` for quick lookup during implementation.

## IR Schema (spec §8)

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

## Type Mapping Table (spec §5.2)

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

## Unsupported Patterns (spec §10)

These must emit clear errors or `// VERTUMNUS: manual binding required` stubs:

- **Lifetimes in public API** — cannot be safely represented in Python
- **`async fn`** — requires `pyo3-asyncio`/`tokio`, out of scope for v1
- **`dyn Trait` return types** — cannot map without runtime type erasure
- **Generic functions not monomorphized** — e.g. `fn foo<T: Display>(x: T)`
- **`unsafe` public functions** — included in IR but flagged; generator emits stub with safety comment
- **Circular type references** — IR represents them, generator may need manual intervention

## Repository Structure

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
│   └── ir.schema.json          # JSON Schema for the IR
├── templates/
│   ├── pyproject.toml.j2       # Build config template
│   ├── lib_rs.j2               # Binding glue template
│   └── pyi.j2                  # Stub file template
├── tests/
│   ├── fixtures/               # Sample Rust crates for integration tests
│   └── integration/            # End-to-end tests
└── docs/
    ├── architecture.md
    ├── type-mapping.md
    └── limitations.md
```

## CLI Interface (spec §5.5)

```
vertumnus wrap <path-to-crate> [--out <dir>] [--package-name <name>] [--dry-run] [--no-build] [--verbose] [--overwrite]
vertumnus inspect <path-to-crate>     # Phase 1 only, dump IR as JSON
vertumnus map <ir.json>               # Phase 2 only, dump annotated IR
vertumnus generate <annotated.json>   # Phase 3 only, emit Rust + stub files
```

## Milestones

- **M1** — Inspector + IR (rustdoc JSON parsing, `vertumnus inspect` works)
- **M2** — Type Mapper (primitives, Vec, HashMap, Option, Result, warnings for unsupported)
- **M3** — Binding Generator (PyO3 glue for fn/struct/enum, .pyi stubs, compiles with cargo check)
- **M4** — Builder + CLI (pyproject.toml, maturin invocation, `vertumnus wrap` end-to-end on 2+ fixtures)
- **M5** — Polish (CI templates, --dry-run/--verbose, docs, integration test suite with 3+ real crates)
