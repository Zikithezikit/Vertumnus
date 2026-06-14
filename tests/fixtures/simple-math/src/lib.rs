//! A simple math library for testing Vertumnus.
//!
//! This crate provides basic arithmetic operations and geometric types
//! to exercise all the major IR features: functions, structs, enums,
//! generics, and impl blocks.

/// Adds two 64-bit integers.
///
/// # Examples
///
/// ```
/// assert_eq!(simple_math::add(2, 3), 5);
/// ```
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}

/// Divides two floating-point numbers.
///
/// Returns `None` if division by zero would occur.
pub fn div(a: f64, b: f64) -> Option<f64> {
    if b == 0.0 {
        None
    } else {
        Some(a / b)
    }
}

/// Computes the length of a vector.
pub fn magnitude(x: f64, y: f64, z: f64) -> f64 {
    (x * x + y * y + z * z).sqrt()
}

/// A 2D point with floating-point coordinates.
#[derive(Debug)]
pub struct Point {
    /// The x-coordinate
    pub x: f64,
    /// The y-coordinate
    pub y: f64,
}

impl Point {
    /// Create a new `Point`.
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// Compute the distance between two points.
    pub fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Move the point by a delta.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

/// Cardinal and intercardinal directions.
#[derive(Debug, PartialEq)]
pub enum Direction {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl Direction {
    /// Returns the direction as a (dx, dy) offset.
    pub fn offset(&self) -> (i32, i32) {
        match self {
            Direction::North => (0, 1),
            Direction::South => (0, -1),
            Direction::East => (1, 0),
            Direction::West => (-1, 0),
            Direction::NorthEast => (1, 1),
            Direction::NorthWest => (-1, 1),
            Direction::SouthEast => (1, -1),
            Direction::SouthWest => (-1, -1),
        }
    }
}

/// A generic wrapper type to test generic handling.
pub struct Wrapper<T> {
    pub inner: T,
}

impl<T> Wrapper<T> {
    pub fn new(inner: T) -> Self {
        Wrapper { inner }
    }
}

/// A result type for fallible operations.
#[derive(Debug, PartialEq)]
pub enum MathError {
    DivisionByZero,
    Overflow,
}

/// Performs integer division with error handling.
pub fn safe_div(a: i64, b: i64) -> Result<i64, MathError> {
    if b == 0 {
        Err(MathError::DivisionByZero)
    } else {
        Ok(a / b)
    }
}

/// A struct with a lifetime — v1 should handle this with a warning.
pub struct Ref<'a> {
    pub value: &'a str,
}
