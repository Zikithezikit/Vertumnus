# Vertumnus — Memory & State

## Current State (2026-06-15)

### Sprint 5 Progress
| Item | Status | Notes |
|---|---|---|
| D2 — Batch wrapping | ✅ Complete | `vertumnus batch wrap` command |
| B2 — User monomorphization hints | ✅ Complete | Config-file monomorphization |
| B3 — Generic type parameter erasure | ✅ Complete | PhantomData<T> erased, PyClass generated |
| C3 — Streaming IR for huge crates | ⏭️ Next | Not yet started |
| E3 — Cross-crate workspace wrapping | ❌ Not started | |
| E4 — Plugin system | ❌ Not started | |

### B3 Implementation Details
- `generic_params: Vec<String>` added to `StructItem`, `EnumItem`, `FunctionItem` in IR
- PhantomData fields detected and filtered from Python-visible mappings
- `are_generics_erasure_safe()` checks if all generic params only appear in PhantomData fields
- Erased generics filled with `()` in inner type reference (e.g., `_crate::Marker<()>`)
- PhantomData type maps to `None`/Native in type parser
- No fixture crate yet; tests are unit-level only

### Known Issues
- No fixture crate for phantom-markers integration test
- `is_generics_erasure_safe` uses string-level heuristics (may false-positive if param name appears in unrelated type strings)

### Commit History (latest)
```
56a4601 docs(plan): mark B3 complete, move next to C3
baa9267 feat(mapper): add generic type parameter erasure for PhantomData (B3)
9fb8694 feat(mapper): add user-provided monomorphization hints (B2)
```

### Memory Conventions
- Update this file at end of each day
- Document blocker rationale if skipping items
- Tag items with priority (P0–P2) when relevant
