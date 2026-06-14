# Known Limitations (v1)

> **Version:** 0.1.0
> **Last updated:** 2026-06-14

## Introduction

Vertumnus v1 focuses on the most common Rust API patterns. It handles simple functions, structs with public fields, C-like enums, and standard library collections. However, several Rust features cannot be automatically mapped to Python and require manual intervention.

## Limitations

### 1. Rust Lifetimes in Public API

**Problem:** Lifetimes like `'a`, `'static`, or `'_` cannot be safely represented in Python because Python's memory model is garbage-collected and has no concept of borrow checking.

**Behavior:** Vertumnus emits a warning during mapping and skips the affected item. The generated code contains a `// VERTUMNUS: manual binding required` comment.

**Workaround:** Either refactor the Rust crate to use owned types (`String` instead of `&str`, `Vec<T>` instead of `&[T]`) or manually write the binding.

**Example of skipped items:**
```rust
pub struct Ref<'a> {
    pub value: &'a str,  // ⚠️ Skipped: lifetime in field
}

pub fn greet(name: &str) -> String {
    // ⚠️ &str is mapped as owned String in generated wrapper
    format!("Hello, {}", name)
}
```

### 2. Async Functions

**Problem:** `async fn` requires an async runtime (tokio, async-std) and `pyo3-asyncio` integration, which adds significant complexity.

**Behavior:** Vertumnus emits a warning and generates a stub.

**Workaround:** Manually implement the binding using `pyo3-asyncio`.

### 3. `dyn Trait` Types

**Problem:** Trait objects require runtime type erasure. Python has no way to statically enforce trait bounds.

**Behavior:** Mapped to `Any` with a warning. The generated function uses `Bound<'_, PyAny>` in the PyO3 signature.

**Workaround:** Manually define the expected Python type in a custom binding.

### 4. Generic Functions (Un-monomorphized)

**Problem:** A function like `fn foo<T: Display>(x: T) -> String` cannot be called from Python without knowing the concrete type.

**Behavior:** The item is flagged with a warning and generated as a stub.

**Workaround:** Either monomorphize manually for expected types or expose concrete wrapper functions.

### 5. Data-Carrying Enums

**Problem:** Enums where variants contain data (e.g., `enum Value { Int(i32), Text(String) }`) cannot be easily represented as simple Python classes.

**Behavior:** C-like enums (no data in variants) work perfectly. Data-carrying enums get a ManualStub with a warning.

**Workaround:** Manually implement the Python class using `PyO3` with appropriate enum dispatch.

### 6. `unsafe` Public Functions

**Problem:** Unsafe functions require the caller to uphold invariants that cannot be expressed in Python.

**Behavior:** The generated wrapper is safe (wrapped in a safe function) but includes a comment flagging the original as unsafe.

**Workaround:** Review the unsafe function's safety requirements and add assertions in the Python binding if needed.

### 7. Circular Type References

**Problem:** Types that reference each other (e.g., `struct A { b: B }` and `struct B { a: A }`) can cause issues in the IR representation and code generation.

**Behavior:** The IR will represent them, but code generation may produce code that doesn't compile if both types need to be registered as `#[pyclass]`.

**Workaround:** Manually adjust the generated code to handle the circular dependency (e.g., use `Py<SomeType>` for one direction).

### 8. Complex Generic Constraints

**Problem:** Traits with complex generic constraints like `fn foo<T: Into<Vec<u8>>>(x: T)` cannot be resolved without knowing the concrete type.

**Behavior:** The function is skipped with a warning.

**Workaround:** Provide concrete wrapper functions.

### 9. Modules and Re-exports

**Problem:** The current inspector uses rustdoc JSON which does not fully preserve module hierarchy. All public items are flattened into a single module.

**Behavior:** All items appear in the top-level Python module.

**Workaround:** Manual module reorganization in the generated code.

### 10. Associated Constants and Type Aliases

**Problem:** `const` items and `type` aliases in Rust's public API are not yet inspected or generated.

**Behavior:** These items are silently ignored.

**Workaround:** Add them manually to the generated stubs if needed.

## Robustness Guarantees

When Vertumnus encounters an unsupported pattern, it **always**:
1. Emits a structured warning with the item's location and a description of the issue
2. Generates a `todo!()` stub with a `// VERTUMNUS: manual binding required` comment
3. Excludes the problematic item from `#[pymodule]` registration so the generated crate still compiles

This means that even for crates with unsupported features, the generated Python package will be **installable** — it simply won't expose those specific items until you manually implement them.

## What Works Well

The following patterns work reliably in v1:

- **Simple functions** with primitive types, `String`, `Vec`, `HashMap`, `Option`, `Result`
- **Structs** with public fields of supported types
- **C-like enums** (no data variants) with methods
- **Methods** taking `&self`, `&mut self`, or owned `self`
- **Fallible functions** returning `Result<T, E>` — mapped to Python exceptions
- **Nested generics** like `Option<Vec<String>>` or `Result<i64, MyError>`
- **Doc comments** — preserved as Python docstrings

## Reporting Issues

If you encounter a Rust API pattern that produces incorrect generated code (not just a warning/stub), please file an issue at the project repository with:
1. The minimal Rust code that demonstrates the issue
2. The generated output
3. The expected behavior
