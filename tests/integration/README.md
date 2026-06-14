# Integration Tests

End-to-end Python tests for Vertumnus-generated bindings.

## Running Tests

```bash
# From repository root
pytest tests/integration/ -v

# Skip automatic fixture building (if already built)
pytest tests/integration/ --skip-build -v

# Run specific test file
pytest tests/integration/test_simple_math.py -v
```

## Prerequisites

1. **Install vertumnus CLI:**
   ```bash
   cargo install vertumnus-cli
   ```

2. **Install pytest:**
   ```bash
   pip install pytest
   ```

3. **Nightly Rust toolchain** (required for rustdoc JSON):
   ```bash
   rustup toolchain install nightly
   ```

## Test Structure

- `conftest.py` — Pytest configuration, auto-builds fixtures before tests
- `test_simple_math.py` — Tests for the simple-math fixture (arithmetic, Point, Direction)
- `test_data_structures.py` — Tests for collections (Vec, HashMap, Option)
- `test_string_utils.py` — Tests for string handling (&str, String, UTF-8)

## Fixtures

The tests use fixtures from `tests/fixtures/`:
- `simple-math/` — Basic types, functions, structs, enums
- `data-structures/` — Collections and nested types
- `string-utils/` — String operations

Each fixture is automatically wrapped with `vertumnus wrap` before tests run (unless `--skip-build` is specified).

## CI Integration

The GitHub Actions workflow runs these tests:

```yaml
- name: Run integration tests
  run: |
    cargo install --path crates/vertumnus-cli
    pip install pytest
    pytest tests/integration/ -v
```

## What Gets Tested

### Type Mapping
- ✅ Primitives (i64, f64, bool)
- ✅ String and &str conversion
- ✅ Vec<T> → list[T]
- ✅ HashMap<K,V> → dict[K,V]
- ✅ Option<T> → T | None
- ✅ Result<T,E> → exception on Err

### API Coverage
- ✅ Free functions
- ✅ Struct construction
- ✅ Struct field access (getters)
- ✅ Instance methods (&self, &mut self)
- ✅ Associated functions (static methods)
- ✅ Enum variants
- ✅ Enum methods

### Edge Cases
- ✅ Division by zero (Option returns None)
- ✅ Error propagation (Result → RuntimeError)
- ✅ Large integers (i64 bounds)
- ✅ Unicode/UTF-8 strings
- ✅ Empty collections
- ✅ Multiple instances of same type

### Unsupported Features (negative tests)
- ✅ Generic types (Wrapper<T>) — should be skipped
- ✅ Lifetimes (Ref<'a>) — should be skipped
- ✅ Async functions — should be skipped (v0.3.0)

## Adding New Tests

1. Create a fixture crate in `tests/fixtures/my-feature/`
2. Add a test file `test_my_feature.py`
3. Import the generated module: `import py_my_feature`
4. Write test classes with pytest

Example:
```python
import pytest

@pytest.fixture(scope="module")
def my_module():
    try:
        import py_my_feature
        return py_my_feature
    except ImportError as e:
        pytest.skip(f"py_my_feature not built: {e}")

class TestMyFeature:
    def test_something(self, my_module):
        result = my_module.my_function(42)
        assert result == expected_value
```

## Troubleshooting

**Issue:** `ImportError: No module named 'py_xxx'`
- **Fix:** Run `vertumnus wrap tests/fixtures/xxx` manually, or ensure conftest.py builds fixtures

**Issue:** `vertumnus: command not found`
- **Fix:** Install with `cargo install vertumnus-cli`

**Issue:** Tests fail with `maturin build failed`
- **Fix:** Ensure nightly toolchain is available: `rustup toolchain install nightly`

**Issue:** Fixture built but import still fails
- **Fix:** Install the built package: `pip install -e tests/fixtures/py-xxx/`

## Test Coverage Goals

- [x] Basic type mapping (v0.3.0)
- [x] Error handling (v0.3.0)
- [x] Edge cases (v0.3.0)
- [ ] Async functions (v0.4.0)
- [ ] Generic monomorphization (v0.4.0)
- [ ] Lifetime erasure patterns (v0.4.0)
- [ ] Performance benchmarks (future)
