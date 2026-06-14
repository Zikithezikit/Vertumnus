# Vertumnus

> **Transform any Rust crate into a Python package — with minimal manual binding work.**

[![CI](https://github.com/your-org/vertumnus/actions/workflows/ci.yml/badge.svg)](https://github.com/your-org/vertumnus/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Vertumnus (Roman god of transformation) automatically generates **idiomatic Python bindings** for Rust crates. Give it a Rust crate, and it produces a pip-installable Python wheel with:

- Python classes for Rust structs and enums
- Python functions for Rust public functions
- Type stubs (`.pyi`) for full IDE and mypy/pyright support
- `__init__.py` with clean re-exports

No PyO3 expertise required. No hand-writing `#[pyclass]` annotations. Just `vertumnus wrap`.

---

## Quick Start

```bash
# Install Vertumnus
cargo install vertumnus-cli

# Generate Python bindings for any Rust crate
vertumnus wrap path/to/rust-crate

# Install the resulting package
pip install ./vertumnus-out/<crate-name>/
```

**That's it.** You now have a fully typed Python package backed by native Rust code.

### Try It in 30 Seconds

Pick any crate with a simple public API — or use one of the included fixtures:

```bash
# Wrap a trivial math crate and test it
vertumnus wrap tests/fixtures/simple-math --no-build
# Inspect the generated Rust glue code in vertumnus-out/
```

---

## Usage

### Primary Command: `vertumnus wrap`

```bash
vertumnus wrap <path-to-crate> [options]
```

| Option | Description |
|---|---|
| `--out <dir>` | Output directory (default: `./vertumnus-out`) |
| `--package-name <name>` | Python package name (default: crate name) |
| `--dry-run` | Inspect and map only; don't write files or build |
| `--no-build` | Generate files but don't compile the wheel |
| `--verbose` | Print IR and mapping decisions |
| `--overwrite` | Overwrite existing output files |

### Pipeline Subcommands

Each phase can be run independently:

```bash
vertumnus inspect <path-to-crate>     # Phase 1: dump IR as JSON
vertumnus map <ir.json>               # Phase 2: dump annotated IR
vertumnus generate <annotated.json>   # Phase 3: emit Rust + .pyi stubs
```

### Worked Example

```bash
# 1. Wrap a real-world crate
vertumnus wrap tests/fixtures/data-structures --verbose

# 2. See what was generated
ls vertumnus-out/data-structures/
#   Cargo.toml  pyproject.toml  src/lib.rs  python/data-structures/
#   data-structures.pyi

# 3. Install and use
cd vertumnus-out/data-structures/
pip install .
python -c "import data_structures; print(dir(data_structures))"
```

---

## How It Works

Vertumnus is a **five-phase pipeline**, each implemented as its own Rust crate:

```
┌─────────────────────┐
│   vertumnus wrap    │
│   (CLI orchestrator)│
└─────────┬───────────┘
          │
┌─────────▼───────────┐
│  1. Inspector       │  Parses crate's public API via rustdoc JSON
│  (vertumnus-inspector) │  → Intermediate Representation (IR)
└─────────┬───────────┘
          │
┌─────────▼───────────┐
│  2. Type Mapper     │  Maps every Rust type to its Python equivalent
│  (vertumnus-mapper) │  → Annotated IR
└─────────┬───────────┘
          │
┌─────────▼───────────┐
│  3. Generator       │  Emits PyO3-annotated Rust glue code
│  (vertumnus-generator)│  + .pyi type stubs + __init__.py
└─────────┬───────────┘
          │
┌─────────▼───────────┐
│  4. Builder         │  Scaffolds pyproject.toml, Cargo.toml additions,
│  (vertumnus-builder) │  invokes maturin to produce .whl
└─────────┬───────────┘
          │
┌─────────▼───────────┐
│  Python Wheel       │  pip-installable, fully typed,
│  (.whl + .pyi)      │  backed by native Rust
└─────────────────────┘
```

### The Intermediate Representation (IR)

The IR is the contract between phases — a versioned JSON schema that describes every public symbol in the crate. This means you can **inspect, modify, or even author IR by hand** and feed it into later phases.

See [`schemas/ir.schema.json`](schemas/ir.schema.json) and [`schemas/annotated_ir.schema.json`](schemas/annotated_ir.schema.json).

---

## Features

### ✅ Supported

| Category | Rust Types |
|---|---|
| **Primitives** | `i8`–`i128`, `u8`–`u128`, `f32`, `f64`, `bool` |
| **Strings** | `String`, `&str` |
| **Collections** | `Vec<T>`, `HashMap<K,V>`, `HashSet<T>`, `[T; N]`, `&[T]` |
| **Option/Result** | `Option<T>` → `T \| None`, `Result<T,E>` → raises on `Err` |
| **Smart pointers** | `Box<T>`, `Arc<T>`, `Rc<T>`, `Cow<'_, T>` |
| **Tuples** | `(A, B, ...)` → `tuple[A, B, ...]` |
| **Structs** | Public fields → `@dataclass`-like Python class |
| **Enums** | C-like enums → Python `IntEnum` or class with variants |
| **Methods** | `impl` blocks on structs/enums → Python methods |
| **Functions** | Public free functions → module-level Python functions |
| **Doc comments** | Preserved as Python docstrings |
| **Type stubs** | `.pyi` files for mypy/pyright |

### ⚠️ Graceful Degradation

When Vertumnus encounters a type it can't handle (lifetimes, `dyn Trait`, `async fn`, generics), it **does not crash**. It emits:

```
// VERTUMNUS: manual binding required
// Reason: lifetime parameter 'a in struct Ref
```

...and logs a warning. You can then fill in the binding manually.

---

## Known Limitations

| Feature | Status | Workaround |
|---|---|---|
| **Lifetimes** (`'a`, `&'a str`) | Skipped with warning | Use owned types (`String`, `Vec<T>`) |
| **`async fn`** | Skipped with warning | Manual `pyo3-asyncio` binding |
| **`dyn Trait`** | Skipped with warning | Manual type erasure |
| **Generic functions** (unmonomorphized) | Skipped with warning | Specify concrete types manually |
| **`unsafe` functions** | Generated as stub | Audit and implement manually |
| **Traits** | Informational only | Not bound to Python |

See [full limitations doc](docs/limitations.md).

---

## Project Status

**All five milestones are complete.** Vertumnus v0.1.0 is a functional, end-to-end binding generator.

| Milestone | Status |
|---|---|
| M1 — Inspector + IR | ✅ |
| M2 — Type Mapper | ✅ |
| M3 — Binding Generator | ✅ |
| M4 — Builder + CLI | ✅ |
| M5 — Polish | ✅ |

- 116 tests passing (unit + doc-tests + integration)
- Zero clippy warnings
- Three fixture crates produce installable wheels
- Runs on tagged release versions (`v*.*.*`)

---

## Installation

### From Source

```bash
git clone https://github.com/your-org/vertumnus.git
cd vertumnus
cargo build --release
# Binary at target/release/vertumnus
```

### From Cargo

```bash
cargo install vertumnus-cli --git https://github.com/your-org/vertumnus.git
```

> **Note:** Nightly Rust is required for `rustdoc JSON` output used during inspection.

---

## Development

### Workspace Layout

```
vertumnus/
├── Cargo.toml                   # Workspace root
├── crates/
│   ├── vertumnus-cli/           # CLI binary (Phase orchestrator)
│   ├── vertumnus-inspector/     # Phase 1: rustdoc JSON parser → IR
│   ├── vertumnus-mapper/        # Phase 2: IR → annotated IR
│   ├── vertumnus-generator/     # Phase 3: annotated IR → Rust + .pyi
│   └── vertumnus-builder/       # Phase 4: maturin invocation
├── schemas/                     # IR JSON Schemas
├── tests/fixtures/              # Sample Rust crates for testing
└── docs/                        # Architecture, type-mapping, limitations
```

### Build & Test

```bash
# Build all crates
cargo build

# Run all tests
cargo test

# Lint
cargo clippy

# Build docs
cargo doc --open
```

### CI

CI runs on tags matching `v*.*.*` only. It builds and tests the full workspace, then runs integration tests against all three fixture crates.

---

## Documentation

| Document | Description |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | Full pipeline architecture and component design |
| [`docs/type-mapping.md`](docs/type-mapping.md) | Complete Rust→Python type mapping reference |
| [`docs/limitations.md`](docs/limitations.md) | Known limitations and workarounds |
| [`schemas/ir.schema.json`](schemas/ir.schema.json) | IR JSON Schema (phase contract) |
| [`schemas/annotated_ir.schema.json`](schemas/annotated_ir.schema.json) | Annotated IR JSON Schema |

---

## License

Vertumnus is licensed under the [MIT License](LICENSE).

---

## Why "Vertumnus"?

> *Vertumnus — Roman god of transformation, seasons, and plant growth. The name reflects the core metaphor: taking something in one form and transforming it, naturally and completely, into another.*

---

## Contributing

Vertumnus is opinionated but not closed. If you encounter a crate that doesn't wrap cleanly, please open an issue with:

- The crate name and version
- The output of `vertumnus inspect <crate> --verbose`
- The generated output (if any)

Pull requests are welcome, especially for expanding the type mapping table or adding fixture crates.
