# Vertumnus

Turn any Rust crate into a Python package — no PyO3 knowledge needed.

```bash
vertumnus wrap path/to/rust-crate            # output goes to path/to/py-rust-crate/
pip install path/to/py-rust-crate/
```

The output is placed **alongside your crate** (in the same parent folder), not in your current working directory. You get a fully typed Python package backed by native Rust code.

---

## Install

```bash
cargo install vertumnus-cli
```

> Nightly Rust required (uses `rustdoc JSON` for crate inspection).

---

## Usage

### Wrap a crate into a Python package

```bash
vertumnus wrap path/to/some-rust-crate
```

The output is placed **alongside the crate**, not in your current directory. If your crate is at `path/to/some-rust-crate`, the generated package lands at `path/to/py-some-rust-crate/`.

```bash
pip install path/to/py-some-rust-crate/
python -c "import py_some_rust_crate; print(dir(py_some_rust_crate))"
```

### Options

| Flag | What it does |
|---|---|
| `--out <dir>` | Output directory (default: alongside the crate as `py-<crate_name>`) |
| `--package-name <name>` | Python package name (default: `py-<crate_name>`) |
| `--dry-run` | Inspect and map only — don't write files or build |
| `--no-build` | Generate binding code, don't compile the wheel |
| `--verbose` | Show detailed decisions as they're made |
| `--overwrite` | Overwrite existing files |

### Pipeline subcommands

Each phase can also be run separately:

```bash
vertumnus inspect path/to/crate      # dump the crate's public API as JSON
vertumnus map ir.json                # show how Rust types → Python types
vertumnus generate annotated.json    # emit binding code and .pyi stubs
```

---

## What gets mapped

| Rust | Python |
|---|---|
| `i8`–`i128`, `u8`–`u128`, `f32`, `f64`, `bool` | `int`, `float`, `bool` |
| `String`, `&str` | `str` |
| `Vec<T>` | `list[T]` |
| `HashMap<K,V>` | `dict[K,V]` |
| `Option<T>` | `T \| None` |
| `Result<T, E>` | `T` (raises `RuntimeError` on error) |
| `struct Point { x: f64, y: f64 }` | `class Point` |
| `enum Direction { North, South }` | `class Direction` with variants |
| `impl` blocks | methods on the class |

Stuff Vertumnus can't handle (lifetimes, `async fn`, `dyn Trait`, generics) gets a `// VERTUMNUS: manual binding required` comment in the generated code — the tool doesn't crash, it just flags it and moves on.

---

## Example

```bash
# A real crate in this repo — output goes alongside it
vertumnus wrap tests/fixtures/simple-math --no-build

ls tests/fixtures/py-simple-math/
#   Cargo.toml  pyproject.toml  src/lib.rs
#   py_simple_math.pyi  python/py_simple_math/

pip install tests/fixtures/py-simple-math/
python -c "
import py_simple_math
print(py_simple_math.add(2, 3))       # 5
print(py_simple_math.div(10.0, 2.0))   # 5.0
p = py_simple_math.Point(1.0, 2.0)
print(p.distance(py_simple_math.Point(4.0, 6.0)))  # 5.0
"
```

---

## Known limitations

| Feature | What happens |
|---|---|
| Lifetimes (`'a`, `&'a str`) | Skipped with a warning |
| `async fn` | Skipped with a warning |
| `dyn Trait` | Skipped with a warning |
| Generic functions (unmonomorphized) | Skipped with a warning |
| `unsafe` functions | Emitted as a stub — review manually |

For full details see [`docs/limitations.md`](docs/limitations.md).


