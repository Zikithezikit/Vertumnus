# Vertumnus Roadmap

## ✅ v0.3.0 — Production Ready (Released 2026-06-14)

**Status:** Complete and published to crates.io

**Achievements:**
- Full pipeline: inspect → map → generate → build
- Real-world crate support (successfully wraps dtolnay/semver)
- 9 critical bug fixes for production use
- Comprehensive type mapping (primitives, collections, Result, Option)
- Dynamic import detection (no unused imports)
- Proper error propagation and PyResult handling
- Generated code is readable and idiomatic
- CI/CD with cross-platform wheel building
- Complete documentation (README, LIMITATIONS.md, docs/)

**Known Limitations:**
- No async function support
- Lifetimes in public API not supported
- Generic functions must be monomorphized manually
- Trait objects (dyn Trait) skipped with warnings

---

## 🚀 v0.4.0 — Async & Generics (Planned)

**Theme:** Expand coverage to async-first crates and improve generic handling

### Priority 1: Async Function Support

**Goal:** Wrap async Rust functions as Python async/await compatible methods

**Dependencies:**
- `pyo3-asyncio` integration
- Runtime selection (tokio, async-std, or runtime-agnostic)

**Tasks:**
- [ ] Detect async functions in inspector (already captured in IR as `is_async: bool`)
- [ ] Map async functions to `async def` in Python stubs
- [ ] Generate PyO3 async bindings using `pyo3-asyncio`
- [ ] Add `pyo3-asyncio` dependency with feature flags
- [ ] Support both `tokio` and `async-std` runtimes via config
- [ ] Test with async fixture crate (tokio-based HTTP client example)
- [ ] Document async limitations (executor requirements, GIL handling)

**Example Output:**
```rust
// Input Rust
pub async fn fetch_data(url: &str) -> Result<String, Error>

// Generated PyO3
#[pyfunction]
fn fetch_data(py: Python, url: String) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        let result = original::fetch_data(&url).await
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;
        Ok(result)
    })
}
```

**Success Criteria:**
- Wrap at least 2 async crates successfully (e.g., `reqwest` wrapper, async file I/O)
- Generated Python code works with `asyncio.run()` and `await`
- Documentation covers runtime setup requirements

---

### Priority 2: Generic Function Monomorphization Hints

**Goal:** Provide escape hatches for common generic patterns without full auto-monomorphization

**Approach:** Configuration file for manual type hints

**Tasks:**
- [ ] Design `vertumnus.toml` schema for generic resolution
- [ ] Allow users to specify monomorphizations: `MyType::from<String>() -> from_string()`
- [ ] Generate multiple Python functions from single generic Rust function
- [ ] Document common patterns (Display, Into<T>, AsRef<T>)

**Example Config:**
```toml
# vertumnus.toml
[generics]
"Container::from" = [
    { T = "String", python_name = "from_string" },
    { T = "Vec<u8>", python_name = "from_bytes" },
]
```

**Success Criteria:**
- At least 3 common generic patterns supported
- Clear error messages when generics cannot be resolved
- Documentation with migration guide for generic-heavy crates

---

### Priority 3: Enhanced Derive Support

**Goal:** Auto-generate `__repr__`, `__eq__`, `__hash__` when Rust traits are derived

**Current State:**
- Generator has stub support for Debug → `__repr__`
- PartialEq checking exists but not fully utilized

**Tasks:**
- [ ] Parse `#[derive(...)]` attributes from rustdoc JSON
- [ ] Map `Debug` → `__repr__` automatically
- [ ] Map `PartialEq` → `__eq__` and `__ne__`
- [ ] Map `Hash` → `__hash__`
- [ ] Map `Ord` → `__lt__`, `__le__`, `__gt__`, `__ge__`
- [ ] Add generator config to opt-out: `derive_python_magic = false`
- [ ] Test with data structure fixtures

**Success Criteria:**
- Python classes feel native with proper `print()` and `==` support
- Opt-in/opt-out mechanism documented
- No performance regression for large structs

---

### Priority 4: Better Lifetime Handling for Common Patterns

**Goal:** Support borrowing patterns that can be safely copied or cloned

**Approach:** Detect and auto-clone for small types

**Tasks:**
- [ ] Identify functions returning `&str` → automatically clone to `String`
- [ ] Identify functions returning `&[T]` where `T: Clone` → clone to `Vec<T>`
- [ ] Add `--strict-lifetimes` flag to disable auto-cloning (for perf-sensitive cases)
- [ ] Document memory implications
- [ ] Test with parser crates that use borrowed returns

**Example:**
```rust
// Input Rust
pub fn get_name(&self) -> &str { &self.name }

// Generated PyO3 (auto-clone)
#[getter]
fn name(&self) -> String {
    self.inner.get_name().to_string()
}
```

**Success Criteria:**
- Reduce "lifetime not supported" warnings by 50%
- Wrap at least 1 previously-blocked crate (e.g., simplified parser)
- Clear performance warnings in docs

---

### Priority 5: Trait Object Support (Basic)

**Goal:** Handle common trait object patterns like `Box<dyn Error>`

**Approach:** Map to Python base classes or runtime type erasure

**Tasks:**
- [ ] Detect `dyn Trait` in return types
- [ ] Map `Box<dyn Error>` → `PyErr` (error trait special case)
- [ ] Map `Box<dyn Iterator>` → Python iterator protocol
- [ ] Emit stubs for other trait objects with manual binding instructions
- [ ] Document which trait objects are supported

**Success Criteria:**
- Error-returning functions with `Box<dyn Error>` work automatically
- Iterator patterns work with Python `for` loops
- Documentation lists supported trait objects

---

## 🔮 v0.5.0 — Advanced Features (Future)

**Potential themes:**
- Custom type mapping plugins
- Incremental re-wrapping (only changed items)
- Support for `#[pyproto]` advanced protocols
- Workspace support (wrap multiple crates at once)
- WASM target support (via pyo3-ffi + wasm-bindgen)

---

## 🐛 Known Issues / Technical Debt

### High Priority
- [ ] Empty `tests/integration/` directory — add actual E2E Python tests
- [ ] Generator doesn't check if inner Rust type derives Debug before adding `#[derive(Debug)]` to wrapper
- [ ] No caching of rustdoc JSON output (regenerates on every run)

### Medium Priority
- [ ] Error messages could show source location (file:line)
- [ ] `--verbose` output is noisy, needs structured logging levels
- [ ] Generated code has some formatting inconsistencies (extra blank lines)

### Low Priority
- [ ] `vertumnus map` and `vertumnus generate` commands not well tested standalone
- [ ] No performance benchmarks for large crates (>1000 public items)
- [ ] CI doesn't test actual installation of generated wheels

---

## 📊 Success Metrics

**v0.4.0 Goals:**
- Wrap 10+ real-world crates successfully (currently 1/10)
- Reduce "manual binding required" warnings by 40%
- Support at least 3 async crates
- Maintain 100% test pass rate
- Keep compile times under 5 seconds for medium crates

**Long-term Vision:**
- Become the default tool for Rust → Python wrapping
- 1000+ crates successfully wrapped
- Featured in PyO3 official documentation
- Used by at least 10 production projects

---

**Last Updated:** 2026-06-14  
**Current Version:** v0.3.0  
**Next Version:** v0.4.0 (async + generics)
