# Type Mapping Reference

> **Version:** 0.1.0
> **Last updated:** 2026-06-14

This document describes how Vertumnus maps Rust types to Python types and which PyO3 strategy is used for each mapping.

## Core Mapping Table

| Rust Type | Python Equivalent | PyO3 Strategy | Status |
|---|---|---|---|
| `i8`, `i16`, `i32`, `i64`, `i128` | `int` | Native (passed by value) | ✅ |
| `u8`, `u16`, `u32`, `u64`, `u128` | `int` | Native (passed by value) | ✅ |
| `f32`, `f64` | `float` | Native | ✅ |
| `bool` | `bool` | Native | ✅ |
| `String` | `str` | Native | ✅ |
| `&str` | `str` | Native (calls `.to_string()`) | ✅ |
| `Vec<T>` | `list[T]` | Native | ✅ |
| `HashMap<K, V>` | `dict[K, V]` | Native | ✅ |
| `HashSet<T>` | `set[T]` | Native (via `set`) | ✅ |
| `Option<T>` | `T \| None` | Native `Option<T>` | ✅ |
| `Result<T, E>` | `T` (raises on `Err`) | `PyResult<T>` with `.map_err()` | ✅ |
| `Box<T>` | `T` | Unwrap | ✅ |
| `Arc<T>` | `T` | Unwrap | ✅ |
| `Rc<T>` | `T` | Unwrap | ✅ |
| `Cow<'_, T>` | `T` | Maps inner type | ✅ |
| `&[T]` | `list[T]` | Native | ✅ |
| `[T; N]` | `list[T]` | Native (warns: fixed-size) | ✅ ⚠️ |
| `(A, B, ...)` | `tuple[A, B, ...]` | Native | ✅ |
| `fn(...) -> ...` | `Callable` | `PyAny` (stub) | ✅ |
| `&T`, `&mut T` | `T` | Dereference (warns if lifetime) | ✅ ⚠️ |
| `struct Foo` | `class Foo` | `#[pyclass]` with inner field | ✅ |
| `enum Foo` (C-like) | `class Foo` (int flags) | `#[pyclass]` with `Clone` | ✅ |
| `enum Foo` (data variants) | ⚠️ Stub | ManualStub with warning | ⚠️ |
| `dyn Trait` | `Any` | ManualStub with warning | ⚠️ |
| `impl Trait` | `Any` | ManualStub with warning | ⚠️ |
| Lifetimes (`'a`, `'static`) | — | Warning + skip field/fn | ⚠️ |
| Generic params (`T: Trait`) | — | Warning + ManualStub | ⚠️ |
| Raw pointers (`*const T`, `*mut T`) | `Any` | ManualStub with warning | ⚠️ |
| `async fn` | — | Warning + ManualStub | ❌ |

## PyO3 Strategies

### Native
Types that map directly to Python primitives via PyO3's `FromPyObject`/`IntoPy` implementations. No wrapper struct needed. Examples: integers, floats, bools, strings, `Vec<T>`, `Option<T>`.

### PyClass (`#[pyclass]`)
Rust structs that become Python classes. The generated code creates a wrapper struct with an `inner` field holding the original Rust value, then provides getter methods for each public field and delegates method calls.

### PyEnum (`#[pyclass]` + `Clone`)
C-like enums (variants with no data) become Python classes with integer values. Methods on the enum are generated via `#[pymethods]` and delegate to the original implementation.

### MapErr (`PyResult<T>`)
`Result<T, E>` return types are mapped to `PyResult<T>` in the generated code. When the Rust function returns `Err(e)`, the generated wrapper converts it to a Python `RuntimeError` via `.map_err(|e| PyRuntimeError::new_err(format!("{e:?}")))`.

### ManualStub
Types that cannot be automatically mapped. The generator emits:
- A `todo!()` stub with a `// VERTUMNUS: manual binding required` comment
- The item is excluded from `#[pymodule]` registration
- A warning is printed during the mapping phase

## Edge Cases

### `&str` in function parameters
`&str` parameters receive a Python `str` which PyO3 converts automatically. In the generated code, the wrapper converts it to `String`.

### `&str` in struct fields
Structs with `&str` fields will warn about lifetime issues and generate a ManualStub.

### Option and Result nesting
`Option<Result<T, E>>` maps to `Optional[T]` (Python) with inner `PyResult<T>` behavior. The generated code handles the nested pattern.

### HashMap with complex keys
While `HashMap<K, V>` maps to `dict[K, V]`, the keys must be hashable in Python. Rust types with custom `Hash` + `Eq` that map to Python classes may cause runtime issues — test manually.

## Warnings During Mapping

The type mapper emits structured warnings for:
- **Lifetimes:** `"Type '&'a str' has lifetime 'a' — lifetimes cannot be safely represented in Python"`
- **Generic parameters:** `"Type 'T' is a generic parameter — manual monomorphization required"`
- **`dyn Trait`:** `"Trait object 'dyn Trait' is not automatically mappable to Python"`
- **`impl Trait`:** `"'impl Trait' return type is not automatically mappable to Python"`
- **Data-carrying enums:** `"Enum 'Foo' has data-carrying variants — requires manual binding"`
- **Fixed-size arrays:** `"Type '[u8; 4]' is a fixed-size array — treating as list[T] but size is not enforced"`
- **Raw pointers:** `"Raw pointer 'const T' is unsafe and not mappable to Python"`
- **`async fn`:** `"Async functions are not supported in v1"`
- **`unsafe`:** `"Unsafe function — generated wrapper is safe but original requires care"`

## Rejected/Unsupported Patterns

The following patterns cause the item to become a ManualStub (not registered in the module):

1. Any function/field type containing an unresolved generic parameter
2. Any function/field type containing a lifetime reference
3. Enum variants with associated data (non-C-like enums)
4. `dyn Trait` types in function signatures or fields
5. `impl Trait` return types
6. Raw pointer types
7. `async fn` signatures
