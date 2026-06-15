# Vertumnus — Scaling Plan

> **Goal:** Make Vertumnus work for arbitrary Rust crates dynamically and scalably.
> **Date:** 2026-06-15
> **Status:** Active — Sprint 5 (A1 ✅, C1 ✅, A2 ✅, B1 ✅, C2 ✅, A3 ✅, D1 ✅, E1 ✅, E2 ✅, D2 ✅, B2 ✅)
> Next: B3 — Generic type parameter erasure

---

## Current Bottlenecks

| Bottleneck | Why It Limits Scale |
|---|---|
| **Nightly-only rustdoc** | Not all crates compile on nightly. Some have `#![feature(...)]` conflicts or pinned stable toolchains. |
| **No generic monomorphization** | `fn foo<T: Display>(x: T)` → always `ManualStub`. Most real crates use generics. |
| **No async support** | ~40% of modern Rust crates export `async fn`. They all become stubs. |
| **Hardcoded stdlib type map** | Only `Vec`, `HashMap`, `Option`, `Result` are known. Ecosystem types like `Bytes`, `Duration`, `PathBuf`, `Instant`, `NonZero*` are unknown. |
| **No transitive dependency handling** | If crate A re-exports `B::Type`, Vertumnus has no way to know about it. |
| **No cross-crate workspace support** | Multi-crate workspaces (e.g., `serde` + `serde_derive`) can't be wrapped as a single package. |
| **Single-threaded pipeline** | Large crates (100+ public items) processed one item at a time. |
| **No caching / incremental mode** | Every run re-inspects, re-maps, re-generates from scratch. |
| **No user-extensible type mapping** | Users can't teach Vertumnus about their dependency's types without editing source code. |

---

## Phase A: Increase Crate Compatibility (wider reach)

### A1 — Config-file type mapping registry

The single highest-impact change. Introduce a `.vertumnus/config.toml` that users can provide alongside any crate, allowing them to map ecosystem types without modifying Vertumnus source code.

```toml
# .vertumnus/config.toml
[type_mappings]
"bytes::Bytes" = { python = "bytes", strategy = "native" }
"chrono::NaiveDate" = { python = "datetime.date", strategy = "manual" }
"url::Url" = { python = "str", strategy = "native" }
"std::time::Duration" = { python = "float", strategy = "native" }
"std::path::PathBuf" = { python = "str", strategy = "native" }
"std::num::NonZeroU64" = { python = "Optional[int]", strategy = "native" }
```

**Effort:** 1–2 days | **Impact:** Lets users wrap 10× more crates immediately

### A2 — Stable Rust fallback (`syn`-based parsing)

Current inspector requires nightly for rustdoc JSON. Add a fallback path using the `syn` crate to parse the Rust source directly.

```rust
// Try nightly first, fall back to syn
if nightly_toolchain_available() {
    run_rustdoc_json(path)
} else {
    // cargo check first, then syn-based AST parsing
    parse_with_syn(path)
}
```

**Effort:** 3–5 days | **Impact:** Works on every crate, not just nightly-compatible

### A3 — Dependency-aware type resolution

When the inspector encounters `url::Url`:
1. Read `Cargo.lock` to find the dependency version
2. Check the user's config registry for a mapping
3. If not found, peek at the dependency's rustdoc JSON too
4. Fall back to `Bound<'_, PyAny>` with a warning

**Effort:** 3–4 days | **Impact:** Handles re-exports and ecosystem types

---

## Phase B: Handle Generics at Scale

### B1 — Auto-detect monomorphization from public API

Rather than requiring users to manually specify monomorphizations, detect them automatically. If a generic `Wrapper<T>` is only ever used as `Wrapper<String>` and `Wrapper<i64>` in the public API's function signatures, generate those two concrete wrappers.

```rust
// If public API only uses these:
pub fn create_string_wrapper() -> Wrapper<String>;
pub fn use_int_wrapper(w: Wrapper<i64>) -> i64;

// Generate:
#[pyclass]
struct WrapperString { inner: _crate::Wrapper<String> }
#[pyclass]
struct WrapperInt { inner: _crate::Wrapper<i64> }
```

**Effort:** 2–3 days | **Impact:** Fewer ManualStubs for generic-heavy crates

### B2 — User-provided monomorphization hints

When auto-detection isn't enough, let users provide explicit monomorphization in `.vertumnus/config.toml`:

```toml
[monomorphize]
"Wrapper<String>" = { python = "WrappedString", strategy = "pyclass" }
"Wrapper<i64>" = { python = "WrappedInt", strategy = "pyclass" }
"Result<Data, MyError>" = { python = "Data", strategy = "maperr" }
```

**Effort:** 1 day | **Impact:** Complements B1 for edge cases

### B3 — Generic type parameter erasure (for simple cases)

For generics where the type parameter doesn't affect functionality (e.g., `PhantomData<T>`, marker types), erase the parameter and generate a single wrapper.

```rust
struct Marker<T>(PhantomData<T>);  // → class Marker: pass (no generic)
```

**Effort:** 1–2 days | **Impact:** Handles common marker/phantom patterns

---

## Phase C: Build Scalability (bigger crates)

### C1 — Parallel pipeline stages

Process IR items concurrently using `rayon`:

```
Inspector → [parallel mapper workers] → [parallel generator workers] → Builder
                 ↑                            ↑
          item1 → Mapper                codegen for item1
          item2 → Mapper                codegen for item2
          ...    concurrently           ...    concurrently
```

The IR items are independent — each can be mapped and (mostly) generated in parallel.

**Effort:** 1 day | **Impact:** 2–5× faster on large crates

### C2 — Incremental caching

Cache the IR and annotated IR on disk, keyed by content hash of `src/` files:

```
.cache/vertumnus/<crate_name>/
  ir.json              # cached IR
  ir.content_hash      # sha256 of all source files
  annotated_ir.json    # cached mapping
```

- Skip inspection if source hasn't changed
- Skip mapping if IR hasn't changed
- 10–50× faster re-wraps during development

**Effort:** 2–3 days | **Impact:** Dramatically faster iteration

### C3 — Streaming IR for huge crates

For crates with 1000+ public items, serialize/deserialize items in batches instead of loading everything into memory at once. Use `serde_json::StreamDeserializer` or a database-backed store.

**Effort:** 2–3 days | **Impact:** Enables wrapping very large crates without OOM

---

## Phase D: Ecosystem Scale (many crates)

### D1 — Community type mapping registry

A community-maintained type mapping registry akin to Homebrew taps:

```bash
vertumnus registry search fast       # search for "fast" mappings
vertumnus registry add bytes::Bytes=bytes
vertumnus registry update            # pull latest community mappings
```

Hosted as a simple GitHub repo with `.toml` files. Version-pinned per crate version. Accepts PRs from anyone.

**Effort:** 3–4 days | **Impact:** Builds a flywheel — more mappings → more working crates → more contributors

### D2 — Batch wrapping

```bash
vertumnus batch wrap ./crates-to-wrap/*/    # wrap all crates in a directory
vertumnus batch wrap tokio hyper reqwest     # wrap multiple crates at once
```

**Effort:** 1–2 days | **Impact:** Enables wrapping entire dependency graphs

### D3 — Compatibility dashboard

A CI job that runs `vertumnus wrap` on the top N crates from crates.io weekly and publishes:
- Which crates produce a valid wheel?
- Which types remain unsupported?
- Trends over time

**Effort:** 2–3 days | **Impact:** Measures progress, surfaces regressions

---

## Phase E: Rich Type System Support

### E1 — Async function support

Use `pyo3-asyncio` to bridge async Rust to Python coroutines:

```rust
#[pyfunction]
fn fetch_data(py: Python<'_>, url: &str) -> PyResult<PyObject> {
    let future = async { _crate::fetch_data(url).await };
    pyo3_asyncio::tokio::future_into_py(py, async move {
        future.await.map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}
```

**Effort:** 3–5 days | **Impact:** Unlocks ~40% of modern Rust crates

### E2 — Data-carrying enum support

Generate Python classes for enums with data:

```rust
enum Shape {
    Circle(f64),
    Rect { width: f64, height: f64 },
}
```

↓

```python
class Shape:
    @staticmethod
    def circle(radius: float) -> Shape: ...
    @staticmethod
    def rect(width: float, height: float) -> Shape: ...
    
    @property
    def is_circle(self) -> bool: ...
    @property
    def is_rect(self) -> bool: ...
```

Using the PyO3 `#[pyclass]` with enum dispatch pattern.

**Effort:** 3–5 days | **Impact:** Handles another major `ManualStub` source

### E3 — Cross-crate workspace wrapping

Detect workspace `Cargo.toml` and wrap all member crates that have `[lib]` into a single Python package with submodules.

**Effort:** 4–5 days | **Impact:** Enables wrapping complex libraries like `tokio`, `serde`, `diesel`

### E4 — User-extensible plugin system

Define a trait for custom type mappers that can be loaded dynamically:

```rust
pub trait TypeMapperPlugin {
    fn name(&self) -> &str;
    fn can_map(&self, type_str: &str, crate_name: &str) -> bool;
    fn map(&self, type_str: &str, location: &str) -> MappedType;
}
```

Load plugins via `dlopen` (`libloading`) or WASM. Ships separately from Vertumnus core.

**Effort:** 5–7 days | **Impact:** The ultimate escape hatch — community can add any type mapping

---

## Quick Wins (ranked by impact/effort)

| # | Change | Impact | Effort |
|---|---|---|---|
| 1 | **A1 — Config file type mappings** | Lets users wrap 10× more crates immediately | 1–2 days |
| 2 | **C1 — Parallel item mapping** | 2–5× faster on large crates | 1 day |
| 3 | **B1 — Auto-detect monomorphization** ✅ | Fewer ManualStubs for generic-heavy crates | 2–3 days → done |
| 4 | **A2 — `syn` fallback for stable Rust** ✅ | Works on every crate, not just nightly | 3–5 days → done |
| 5 | **C2 — Incremental cache** ✅ | 10–50× faster re-wraps | 2–3 days → done |
| 6 | **A3 — Dependency-aware type resolution** ✅ | Handles re-exports and dependency types | 3–4 days → done |
| 7 | **E1 — Async function support** | Unlocks ~40% of modern Rust crates | 3–5 days |
| 8 | **E2 — Data-carrying enum support** | Handles another major ManualStub source | 3–5 days |
| 9 | **D1 — Community type registry** | Builds a flywheel effect | 3–4 days |
| 10 | **D2 — Batch wrapping** | Processes many crates at once | 1–2 days |

---

## Recommended Ordering

### Sprint 1: "Works on more crates" (A1, C1)
1. Config file type mappings (`A1`) — biggest ROI per line of code
2. Parallel pipeline (`C1`) — free speedup

### Sprint 2: "Works everywhere" (A2, B1, C2)
3. `syn` fallback for stable Rust (`A2`)
4. Auto-detect monomorphization (`B1`)
5. Incremental cache (`C2`)

### Sprint 3: "Knows the ecosystem" (A3, D1)
6. Dependency-aware type resolution (`A3`)
7. Community type mapping registry (`D1`)

### Sprint 4: "Rich types" (E1, E2)
8. Async function support (`E1`)
9. Data-carrying enum support (`E2`)

### Sprint 5: "At scale" (D2, E3, B2, B3, C3, E4)
10. Batch wrapping, workspace support, plugin system, remaining items

---

## Success Metrics

| Metric | Current | Target (3 months) |
|---|---|---|
| Crates that wrap successfully | 3/3 fixtures | 50/100 top crates from crates.io |
| Items that produce valid bindings (vs ManualStub) | ~70% | >90% |
| Time to wrap a large crate (100+ items) | ~30s | <5s |
| Time to re-wrap after a source change | ~30s | <1s (cached) |
| Community type mappings contributed | 0 | 50+ |
