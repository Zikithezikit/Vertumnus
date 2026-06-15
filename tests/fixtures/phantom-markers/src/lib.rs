//! A crate for testing Vertumnus generic type parameter erasure (B3).
//!
//! Contains marker types using `PhantomData`, types where generics ARE used
//! in real fields (non-erasure-safe), and mixed cases.

use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// Erasure-safe: generic param only appears in PhantomData
// ---------------------------------------------------------------------------

/// A marker type — generic param T is only used in PhantomData.
/// Should generate a PyClass wrapper with T erased.
pub struct Marker<T> {
    pub _phantom: PhantomData<T>,
}

impl<T> Marker<T> {
    pub fn new() -> Self {
        Marker {
            _phantom: PhantomData,
        }
    }
}

/// A unit-like marker with a PhantomData field.
pub struct UnitMarker<T>(PhantomData<T>);

impl<T> UnitMarker<T> {
    pub fn new() -> Self {
        UnitMarker(PhantomData)
    }
}

// ---------------------------------------------------------------------------
// NOT erasure-safe: generic param appears in real fields
// ---------------------------------------------------------------------------

/// A generic container where T appears in a real field.
/// Should still get a ManualStub.
pub struct Container<T> {
    pub value: T,
}

impl<T> Container<T> {
    pub fn new(value: T) -> Self {
        Container { value }
    }
}

// ---------------------------------------------------------------------------
// Mixed: PhantomData + real generic field — NOT erasure-safe
// ---------------------------------------------------------------------------

/// A type that has both a PhantomData marker and a real generic field.
/// Should get a ManualStub because T appears in `inner`.
pub struct Mixed<T> {
    pub _marker: PhantomData<T>,
    pub inner: T,
}

impl<T> Mixed<T> {
    pub fn new(inner: T) -> Self {
        Mixed {
            _marker: PhantomData,
            inner,
        }
    }
}

// ---------------------------------------------------------------------------
// Multiple type params: one erased, one real — NOT erasure-safe
// ---------------------------------------------------------------------------

/// Two generic params: K only in PhantomData, V in a real field.
/// Should get a ManualStub because V is not erased.
pub struct Keyed<K, V> {
    pub _key_marker: PhantomData<K>,
    pub value: V,
}

impl<K, V> Keyed<K, V> {
    pub fn new(value: V) -> Self {
        Keyed {
            _key_marker: PhantomData,
            value,
        }
    }
}

// ---------------------------------------------------------------------------
// Fully erased: two PhantomData params
// ---------------------------------------------------------------------------

/// Two generic params, both only in PhantomData. Should be erasure-safe.
pub struct DualMarker<A, B> {
    pub _a: PhantomData<A>,
    pub _b: PhantomData<B>,
}

impl<A, B> DualMarker<A, B> {
    pub fn new() -> Self {
        DualMarker {
            _a: PhantomData,
            _b: PhantomData,
        }
    }
}
