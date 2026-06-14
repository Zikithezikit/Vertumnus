---
description: >-
  Primary agent for implementing the Vertumnus Rust-to-Python binding framework.
  Use when building features, fixing bugs, or adding infrastructure to any
  Vertumnus crate (inspector, mapper, generator, builder, CLI).
mode: primary
---

You are implementing **Vertumnus** — a framework that transforms Rust crates into Python packages using PyO3 + Maturin.

## Core principles

1. **Start with the spec.** Before implementing anything, re-read `@spec` for the architecture overview, IR schema, type mapping table, and milestone definitions. Every component must match the spec.
2. **Follow the milestone order.** M1 (Inspector + IR) → M2 (Type Mapper) → M3 (Binding Generator) → M4 (Builder + CLI) → M5 (Polish). Do not skip ahead.
3. **Workspace structure.** The repo uses a Cargo workspace under `crates/`: `vertumnus-cli`, `vertumnus-inspector`, `vertumnus-mapper`, `vertumnus-generator`, `vertumnus-builder`. Each crate has its own `Cargo.toml`.
4. **Code quality.** Run `cargo check` and `cargo test` after every significant change. Keep generated code readable, not machine-looking. No unwrap() in library code — use proper error handling with `thiserror` or `anyhow`.
5. **IR is the contract.** The Intermediate Representation (see spec §8) must remain stable and versioned. Any schema changes must be reflected in `schemas/ir.schema.json` and all downstream phases.
6. **Opinionated, not closed.** Defaults should work out of the box, but always provide escape hatches for complex cases. Unsupported types get `// VERTUMNUS: manual binding required` stubs, not silent failures.

## Development workflow

- `cargo build` — compile all workspace crates
- `cargo check` — fast type-check
- `cargo test` — run Rust tests
- `cargo clippy` — lint
- `cargo doc --open` — build docs
- `cargo +nightly rustdoc -- -Z unstable-options --output-format json` — inspect a crate's public API (the Inspector's input)

## References

- `@spec` — VERTUMNUS_SPEC.md with full architecture, IR schema, type mappings, milestones
- `@pyo3` — PyO3 crate for binding patterns
- `@maturin` — Maturin build tool for wheel packaging

## Key design decisions (from spec)

- **rustdoc JSON** for API inspection (v1), fall back to `syn` if needed
- **Maturin + PyO3** as the binding backend
- **JSON** as the external IR serialization format
- **CLI** with `clap`: `vertumnus wrap`, `inspect`, `map`, `generate` subcommands
- **Type mapping** follows the table in spec §5.2 — Result::Err → Python RuntimeError, unsupported lifetimes → skip with warning
