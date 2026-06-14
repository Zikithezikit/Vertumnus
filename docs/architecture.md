# Vertumnus Architecture

> **Version:** 0.1.0
> **Last updated:** 2026-06-14

## Overview

Vertumnus is a toolchain that transforms any Rust crate into a Python package with minimal manual binding work. It consists of five pipeline phases, each implemented as a separate crate in a Cargo workspace.

## Pipeline Phases

```
┌─────────────────────────────────────────────────────┐
│                    vertumnus CLI                    │
│           (entry point: `vertumnus wrap`)           │
└──────────────────────┬──────────────────────────────┘
                       │
         ┌─────────────▼──────────────┐
         │     Inspector (Phase 1)    │
         │  Parses crate public API   │
         │  via rustdoc JSON          │
         └─────────────┬──────────────┘
                       │  → Intermediate Representation (IR)
         ┌─────────────▼──────────────┐
         │   Type Mapper (Phase 2)    │
         │  Maps Rust types → Python  │
         │  types + PyO3 strategies   │
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
         │  Scaffolds build config +  │
         │  invokes maturin build     │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │    Output Package          │
         │  Installable Python wheel  │
         │  + type stubs + metadata   │
         └────────────────────────────┘
```

## Component Details

### 1. vertumnus-inspector (Phase 1)

**Crate:** `crates/vertumnus-inspector/`

**Responsibility:** Parse a Rust crate and produce a structured Intermediate Representation (IR) of its public API.

**Inputs:** Path to a Rust crate directory containing `Cargo.toml`.

**Outputs:** An `IntermediateRepresentation` (IR) document describing public functions, structs, enums, traits, and impl blocks.

**Implementation:**
- Uses `cargo doc --output-format json` (rustdoc JSON, nightly feature) for API extraction
- Falls back to direct file parsing for basic crate metadata
- Handles: functions (with signatures, generics, lifetimes), structs (with fields), enums (with variants), traits (informational), impl blocks (method grouping)

**Key types:**
- `IntermediateRepresentation` — top-level IR document with version, crate info, items
- `IrItem` — untagged enum covering `FunctionItem`, `StructItem`, `EnumItem`, `TraitItem`, `ImplBlockItem`
- Function inputs/outputs, struct fields, enum variants with full type string representation

### 2. vertumnus-mapper (Phase 2)

**Crate:** `crates/vertumnus-mapper/`

**Responsibility:** For each type in the IR, decide how it maps to Python and what PyO3 strategy to use.

**Inputs:** IR JSON (from Phase 1).

**Outputs:** Annotated IR — each symbol decorated with Python type mapping and PyO3 construct.

**Key components:**
- **Type string parser** (`type_parser.rs`) — recursive descent parser that analyzes Rust type strings and determines the mapping strategy
- **Mapper** (`mapper.rs`) — walks IR items and produces annotated items with type mappings

**Mapping outputs:**
- Each item gets a `TypeMapping` with a `PyO3Strategy` (Native, PyClass, PyEnum, MapErr, ManualStub) and a list of warnings for unsupported patterns

### 3. vertumnus-generator (Phase 3)

**Crate:** `crates/vertumnus-generator/`

**Responsibility:** Emit Rust glue code and Python stubs from the annotated IR.

**Outputs:**
- `src/lib.rs` — PyO3-annotated Rust code with `#[pyfunction]`, `#[pyclass]`, `#[pymodule]`
- `<package_name>.pyi` — Python type stub for IDEs and type checkers
- `python/<package_name>/__init__.py` — thin Python shim re-exporting from the native module

**Key components:**
- **Generator** (`generator.rs`) — Orchestrates code generation, groups methods by type
- **Codegen** (`codegen.rs`) — Generates Rust/PyO3 code for functions, structs, enums, traits
- **Stubs** (`stubs.rs`) — Generates Python `.pyi` and `__init__.py` files

**Generated patterns:**
- Free functions → `#[pyfunction]` wrappers with `PyResult` for fallible, `Option<T>` for nullable
- Structs → `#[pyclass]` with inner delegation, field getters, `#[pymethods]` for methods
- C-like enums → `#[pyclass]` with method dispatch
- Unsupported types → `todo!()` stubs with `// VERTUMNUS: manual binding required` comments

### 4. vertumnus-builder (Phase 4)

**Crate:** `crates/vertumnus-builder/`

**Responsibility:** Set up and invoke the build system to produce a distributable Python package.

**Outputs:**
- `pyproject.toml` — Python project configuration with maturin build backend
- `Cargo.toml` — Rust crate configuration with pyo3 dependency and path dep to original crate
- `.github/workflows/build.yml` — Optional CI workflow template
- Built `.whl` file via `maturin build --release`

**Key functions:**
- `scaffold_all()` — Writes pyproject.toml and Cargo.toml
- `scaffold_ci()` — Writes GitHub Actions CI workflow
- `run_maturin_build()` — Executes maturin build in the output directory
- `run_maturin_develop()` — Installs the package locally via maturin develop

### 5. vertumnus-cli (CLI)

**Crate:** `crates/vertumnus-cli/`

**Responsibility:** Orchestrate all phases and provide a developer-facing interface.

**Commands:**
- `vertumnus wrap <path>` — Full pipeline: inspect → map → generate → build
- `vertumnus inspect <path>` — Phase 1 only, dump IR as JSON
- `vertumnus map <ir.json>` — Phase 2 only, dump annotated IR
- `vertumnus generate <annotated.json>` — Phase 3 only, emit Rust + stub files

**Key flags:**
- `--dry-run` — Inspect and map only; do not write files or build
- `--verbose` — Print per-step progress and mapping decisions
- `--no-build` — Generate files but do not invoke maturin
- `--overwrite` — Overwrite existing output files

## Data Flow

1. **IR (Intermediate Representation):** JSON document with versioned schema (`schemas/ir.schema.json`). The contract between inspector and downstream phases. Contains raw Rust API structure without Python-specific annotations.

2. **Annotated IR:** JSON document with versioned schema (`schemas/annotated_ir.schema.json`). Each IR item is decorated with Python type mappings (target Python type, PyO3 strategy, warnings).

## Workspace Layout

```
vertumnus/
├── Cargo.toml                  # Workspace root (resolver = "2")
├── crates/
│   ├── vertumnus-cli/          # CLI binary
│   ├── vertumnus-inspector/    # Phase 1: IR extraction
│   ├── vertumnus-mapper/       # Phase 2: type mapping
│   ├── vertumnus-generator/    # Phase 3: code generation
│   └── vertumnus-builder/      # Phase 4: build + scaffolding
├── schemas/
│   ├── ir.schema.json          # IR JSON Schema v0.1
│   └── annotated_ir.schema.json # Annotated IR JSON Schema v0.1
├── templates/                  # Template files (future use)
├── tests/
│   ├── fixtures/               # Sample Rust crates for testing
│   └── integration/            # End-to-end integration tests
└── docs/                       # Documentation
```

## Tech Stack

| Layer | Technology |
|---|---|
| CLI tool | Rust + `clap` (derive API) |
| API inspection | rustdoc JSON (nightly feature) |
| Type mapping | Custom recursive descent parser |
| Code generation | String templates in Rust |
| Binding runtime | `pyo3` 0.22 |
| Build/package | `maturin` >=1.0 |
| Testing | `cargo test` (Rust), `pytest` (Python) |
| Serialization | `serde` + `serde_json` |
