# Vertumnus Limitations and Capabilities

This document describes what Vertumnus v0.3.0 can and cannot handle when wrapping Rust crates.

## ✅ Successfully Supported

### Types
- ✅ Primitives (`i8`-`i128`, `u8`-`u128`, `f32`, `f64`, `bool`, `char`)
- ✅ `String`, `&str`
- ✅ `Vec<T>`, `HashMap<K,V>`, `HashSet<T>`
- ✅ `Option<T>`, `Result<T, E>`
- ✅ Tuples `(A, B, C)`
- ✅ Slices `&[T]` (fixed in v0.3.0)
- ✅ Arrays `[T; N]`
- ✅ Custom structs (as `#[pyclass]`)
- ✅ C-like enums (as `#[pyclass]`)
- ✅ `std::cmp::Ordering` (converted to `i8`)
- ✅ Fully-qualified paths like `alloc::vec::Vec<T>`

### Functions
- ✅ Free functions
- ✅ Associated functions (static methods)
- ✅ Instance methods (`&self`, `&mut self`, `self`)
- ✅ Fallible functions returning `Result<T, E>` (mapped to `PyResult<T>`)
- ✅ Functions with `Self` parameters
- ✅ Methods returning wrapped types

### Features
- ✅ Automatic type conversion (Rust ↔ Python)
- ✅ Error mapping (`Result::Err` → Python exceptions)
- ✅ Doc comment preservation
- ✅ Bidirectional enum conversions
- ✅ Struct field getters
- ✅ Vec of wrapper types with element mapping
- ✅ Non-exhaustive enums with wildcard arms
- ✅ Dynamic import detection (no unused imports)

## ❌ Known Limitations (v0.3.0)

### Lifetimes
**Status:** Not supported

Rust lifetimes cannot be represented in Python. Functions with lifetime parameters in their signature are skipped or generate stubs.

**Example (capfile crate):**
```rust
// ❌ Cannot wrap
pub fn parse(input: &[u8]) -> Result<(Self, &[u8]), Error>
//                                           ^^^^^ borrowed return type
```

**Reason:** The returned `&[u8]` borrows from the input, which requires lifetime tracking. Python's garbage collector doesn't support Rust's lifetime semantics.

**Workaround:** Manually implement wrappers that copy data or use `Py<PyBytes>`.

### Async Functions
**Status:** Not supported in v0.3.0

Async Rust functions (`async fn`) require `pyo3-asyncio` integration.

**Example:**
```rust
// ❌ Cannot wrap
pub async fn fetch_data() -> Result<Vec<u8>, Error>
```

**Future:** Planned for v0.4.0 with `pyo3-asyncio` support.

### Generic Functions
**Status:** Limited support

Generic functions without monomorphization cannot be wrapped directly.

**Example:**
```rust
// ❌ Cannot wrap directly
pub fn process<T: Display>(value: T) -> String

// ✅ Can wrap if monomorphized
pub fn process_string(value: String) -> String
```

**Workaround:** Create type-specific wrappers in your Rust crate before running Vertumnus.

### Trait Objects
**Status:** Not supported

`dyn Trait` return types and trait bounds cannot be mapped to Python.

**Example:**
```rust
// ❌ Cannot wrap
pub fn get_handler() -> Box<dyn Handler>
```

**Reason:** Python doesn't have Rust's trait system. Each concrete type would need individual wrapping.

### Complex Nested Types
**Status:** Limited support

Deeply nested or unusual type combinations may not map correctly.

**Example:**
```rust
// ⚠️ May not work
pub fn complex() -> Vec<HashMap<String, Result<Option<Box<dyn Trait>>, Error>>>
```

**Workaround:** Create intermediate types with clearer boundaries.

### Unsafe Functions
**Status:** Warning emitted

Unsafe functions are included but flagged with safety comments.

**Example:**
```rust
// ⚠️ Wrapped with warning comment
pub unsafe fn raw_operation(ptr: *mut u8)
```

**Reason:** Python code cannot reason about Rust's safety invariants.

### Missing Types in IR
**Status:** Inspector limitation

If rustdoc JSON doesn't expose a type (due to visibility, path issues, or macro-generated code), Vertumnus cannot find it.

**Example (capfile crate):**
```rust
// ❌ Not found in public API
pub struct PcapHeader { ... }  // May be in submodule or re-exported differently
```

**Workaround:** Ensure types are properly re-exported in `lib.rs` with `pub use`.

## 📊 Compatibility Matrix

| Crate Type | Support Level | Notes |
|------------|---------------|-------|
| Pure data structures | ✅ Excellent | Structs, enums, basic methods |
| Parser libraries (with lifetimes) | ❌ Poor | Requires borrowed return types |
| Computational libraries | ✅ Excellent | Math, algorithms, transformations |
| Async I/O | ❌ None | Needs `pyo3-asyncio` (future) |
| System APIs (unsafe) | ⚠️ Partial | Manual review required |
| Builder patterns | ✅ Good | Works well with owned types |

## 🎯 Best Practices

### Design Rust APIs for Python Wrapping

1. **Avoid lifetimes in public API**
   - Use `Vec<u8>` instead of `&[u8]` in return types
   - Clone data when necessary
   
2. **Use concrete types over generics**
   - Provide monomorphized functions for common types
   
3. **Return owned data**
   - Prefer `String` over `&str` in return positions
   - Use `Vec<T>` instead of slices when crossing FFI boundary

4. **Keep error types simple**
   - Use `anyhow::Error` or simple enum errors
   - Avoid complex nested error types

5. **Re-export types clearly**
   ```rust
   // lib.rs
   pub use crate::internal::PublicType;
   ```

## 📈 Success Stories

### ✅ dtolnay/semver (v1.0.28)
**Status:** Fully wrapped with 0 compilation errors

- Complex enum conversions (Op with non-exhaustive)
- Result-returning constructors (Version::parse)
- Ordering comparisons
- Vec of wrapper types
- 270KB Python wheel generated

**Command:**
```bash
vertumnus wrap ./semver
✅ Built wheel: py_semver-1.0.28-cp312-cp312-manylinux_2_34_x86_64.whl
```

### ⚠️ Zikithezikit/capfile (v0.1.2)
**Status:** Partial - blocked by lifetimes

**Issues:**
- Functions return borrowed slices `&[u8]` with lifetimes
- Parser pattern requires zero-copy parsing
- Types in submodules not properly re-exported

**Recommendation:** Not suitable for direct wrapping. Requires API redesign to use owned types.

## 🔮 Future Improvements

### Planned for v0.4.0
- [ ] Async function support via `pyo3-asyncio`
- [ ] Better generic function handling
- [ ] Lifetime erasure for common patterns
- [ ] Trait object support for common cases

### Under Consideration
- [ ] Custom type mapping configuration file
- [ ] Manual override hooks for complex types
- [ ] Builder API for incremental wrapping
- [ ] Support for more PyO3 features (`#[pyproto]`, etc.)

---

**Last Updated:** 2026-06-14
**Vertumnus Version:** 0.3.0
