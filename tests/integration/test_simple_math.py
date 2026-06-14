"""
End-to-end integration tests for simple-math fixture.

Tests that the generated Python bindings work correctly when imported.
"""

import pytest
import sys
import os
from pathlib import Path

# Add the parent directory to allow importing the generated package
fixture_dir = Path(__file__).parent.parent / "fixtures"
sys.path.insert(0, str(fixture_dir))


@pytest.fixture(scope="module")
def simple_math():
    """
    Import the wrapped simple-math package.
    
    Assumes vertumnus has already been run on the fixture:
    vertumnus wrap tests/fixtures/simple-math
    """
    try:
        # The package name has underscore, not hyphen
        import py_simple_math as pkg
        return pkg
    except ImportError as e:
        pytest.skip(f"py_simple_math not built yet: {e}")


class TestFunctions:
    """Test free functions from the Rust crate."""
    
    def test_add_integers(self, simple_math):
        assert simple_math.add(2, 3) == 5
        assert simple_math.add(-10, 5) == -5
        assert simple_math.add(0, 0) == 0
    
    def test_div_with_result(self, simple_math):
        result = simple_math.div(10.0, 2.0)
        assert result == 5.0
    
    def test_div_with_none(self, simple_math):
        result = simple_math.div(10.0, 0.0)
        assert result is None
    
    def test_magnitude(self, simple_math):
        # 3-4-5 triangle
        result = simple_math.magnitude(3.0, 4.0, 0.0)
        assert abs(result - 5.0) < 1e-10
        
        # Unit vector
        result = simple_math.magnitude(1.0, 0.0, 0.0)
        assert abs(result - 1.0) < 1e-10


class TestPoint:
    """Test the Point struct and its methods."""
    
    def test_point_construction(self, simple_math):
        p = simple_math.Point(1.0, 2.0)
        assert p.x == 1.0
        assert p.y == 2.0
    
    def test_point_new(self, simple_math):
        # Point.new() is a static method, but PyO3 classes use __init__ instead
        # Test the regular constructor instead
        p = simple_math.Point(3.0, 4.0)
        assert p.x == 3.0
        assert p.y == 4.0
    
    def test_point_distance(self, simple_math):
        p1 = simple_math.Point(0.0, 0.0)
        p2 = simple_math.Point(3.0, 4.0)
        
        dist = p1.distance(p2)
        assert abs(dist - 5.0) < 1e-10
        
        # Distance to self should be zero
        assert abs(p1.distance(p1)) < 1e-10
    
    def test_point_translate(self, simple_math):
        p = simple_math.Point(1.0, 2.0)
        p.translate(5.0, -3.0)
        
        assert abs(p.x - 6.0) < 1e-10
        assert abs(p.y - (-1.0)) < 1e-10
    
    def test_point_field_access(self, simple_math):
        p = simple_math.Point(10.0, 20.0)
        
        # Fields should be readable
        assert p.x == 10.0
        assert p.y == 20.0


class TestDirection:
    """Test the Direction enum."""
    
    def test_direction_variants(self, simple_math):
        """Test that all enum variants are accessible."""
        north = simple_math.Direction.North
        south = simple_math.Direction.South
        east = simple_math.Direction.East
        west = simple_math.Direction.West
        
        assert north is not None
        assert south is not None
        assert east is not None
        assert west is not None
    
    def test_direction_offset(self, simple_math):
        """Test the offset method on enum variants."""
        north = simple_math.Direction.North
        offset = north.offset()
        assert offset == (0, 1)
        
        east = simple_math.Direction.East
        offset = east.offset()
        assert offset == (1, 0)
        
        northeast = simple_math.Direction.NorthEast
        offset = northeast.offset()
        assert offset == (1, 1)


class TestErrorHandling:
    """Test Result-returning functions and error propagation."""
    
    def test_safe_div_success(self, simple_math):
        result = simple_math.safe_div(10, 2)
        assert result == 5
        
        result = simple_math.safe_div(7, 3)
        assert result == 2  # Integer division
    
    def test_safe_div_division_by_zero(self, simple_math):
        with pytest.raises(RuntimeError, match="DivisionByZero"):
            simple_math.safe_div(10, 0)
    
    def test_math_error_enum(self, simple_math):
        """Test that MathError enum is exposed."""
        # The enum should be accessible even if we can't construct it directly
        assert hasattr(simple_math, 'MathError')


class TestTypeAnnotations:
    """Test that .pyi stub file provides correct type hints."""
    
    def test_stub_file_exists(self):
        stub_path = fixture_dir / "py-simple-math" / "py_simple_math.pyi"
        # This may not exist if --no-build was used, so we make it optional
        if stub_path.exists():
            content = stub_path.read_text()
            
            # Check for key type annotations
            assert "def add(a: int, b: int) -> int:" in content
            assert "class Point:" in content
            # Direction is an IntEnum, check for that pattern
            assert "class Direction(IntEnum):" in content or "class Direction:" in content


class TestEdgeCases:
    """Test edge cases and boundary conditions."""
    
    def test_large_integers(self, simple_math):
        # Test with i64 max values
        result = simple_math.add(2**62, 1)
        assert result == 2**62 + 1
    
    def test_negative_floats(self, simple_math):
        result = simple_math.magnitude(-3.0, -4.0, 0.0)
        assert abs(result - 5.0) < 1e-10
    
    def test_multiple_points(self, simple_math):
        """Test that multiple Point instances don't interfere."""
        points = [simple_math.Point(float(i), float(i*2)) for i in range(10)]
        
        for i, p in enumerate(points):
            assert p.x == float(i)
            assert p.y == float(i*2)


class TestUnsupportedFeatures:
    """Test that unsupported features are handled gracefully."""
    
    def test_generic_wrapper_skipped(self, simple_math):
        """Wrapper<T> should be skipped with a warning."""
        # Generic types without monomorphization should not appear
        assert not hasattr(simple_math, 'Wrapper')
    
    def test_lifetime_struct_skipped(self, simple_math):
        """Ref<'a> should be skipped with a warning."""
        # Structs with lifetimes should not appear
        assert not hasattr(simple_math, 'Ref')


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
