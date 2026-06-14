"""
End-to-end integration tests for data-structures fixture.

Tests collections, nested types, and more complex scenarios.
"""

import pytest
import sys
from pathlib import Path

fixture_dir = Path(__file__).parent.parent / "fixtures"
sys.path.insert(0, str(fixture_dir))


@pytest.fixture(scope="module")
def data_structures():
    """Import the wrapped data-structures package."""
    try:
        import py_data_structures as pkg
        return pkg
    except ImportError as e:
        pytest.skip(f"py_data_structures not built yet: {e}")


class TestVecHandling:
    """Test that Vec<T> is properly mapped to list[T]."""
    
    def test_sum_list(self, data_structures):
        """Test function that takes Vec<i64>."""
        result = data_structures.sum_list([1, 2, 3, 4, 5])
        assert result == 15
        
        result = data_structures.sum_list([])
        assert result == 0
    
    def test_unique_sorted(self, data_structures):
        """Test function that takes and returns Vec<i64>."""
        result = data_structures.unique_sorted([3, 1, 2, 1, 3])
        assert result == [1, 2, 3]
        
        result = data_structures.unique_sorted([])
        assert result == []
    
    def test_flatten_options(self, data_structures):
        """Test Vec<Option<T>> handling."""
        result = data_structures.flatten_options([1, None, 2, None, 3])
        assert result == [1, 2, 3]


class TestHashMapHandling:
    """Test that HashMap<K,V> is properly mapped to dict[K,V]."""
    
    def test_word_frequencies(self, data_structures):
        """Test function returning HashMap."""
        result = data_structures.word_frequencies("hello world hello")
        assert result == {"hello": 2, "world": 1}
    
    def test_lookup_or_default(self, data_structures):
        """Test function that takes HashMap."""
        data = {"a": 10, "b": 20}
        
        result = data_structures.lookup_or_default(data, "a", 999)
        assert result == 10
        
        result = data_structures.lookup_or_default(data, "missing", 999)
        assert result == 999
    
    def test_merge_maps(self, data_structures):
        """Test HashMap merging."""
        map_a = {"a": 1, "b": 2}
        map_b = {"b": 20, "c": 30}
        
        result = data_structures.merge_maps(map_a, map_b)
        assert result["a"] == 1
        assert result["b"] == 20  # b overwrites
        assert result["c"] == 30


class TestHashSetHandling:
    """Test that HashSet<T> is properly mapped."""
    
    def test_unique_words(self, data_structures):
        """Test function returning HashSet."""
        result = data_structures.unique_words("hello world hello")
        assert len(result) == 2
        assert "hello" in result
        assert "world" in result
    
    def test_intersect_sets(self, data_structures):
        """Test function taking two HashSets."""
        result = data_structures.intersect_sets({1, 2, 3}, {2, 3, 4})
        assert result == [2, 3]  # Returned as sorted Vec


class TestOptionHandling:
    """Test that Option<T> is properly mapped to T | None."""
    
    def test_first_and_last_some(self, data_structures):
        """Test function returning Some(tuple)."""
        result = data_structures.first_and_last([1, 2, 3, 4, 5])
        assert result == (1, 5)
    
    def test_first_and_last_none(self, data_structures):
        """Test function returning None."""
        result = data_structures.first_and_last([])
        assert result is None


class TestTupleHandling:
    """Test tuple type mapping."""
    
    def test_unzip_pairs(self, data_structures):
        """Test function with tuple types."""
        pairs = [("a", 1), ("b", 2), ("c", 3)]
        strings, numbers = data_structures.unzip_pairs(pairs)
        
        assert strings == ["a", "b", "c"]
        assert numbers == [1, 2, 3]


class TestStructs:
    """Test struct wrapping and methods."""
    
    def test_data_store_creation(self, data_structures):
        """Test DataStore construction."""
        store = data_structures.DataStore("test_store")
        assert store.name == "test_store"
    
    def test_data_store_operations(self, data_structures):
        """Test DataStore methods."""
        store = data_structures.DataStore("my_store")
        
        store.add_value(10)
        store.add_value(20)
        store.add_value(30)
        
        assert store.total() == 60
        
        avg = store.average()
        assert avg is not None
        assert abs(avg - 20.0) < 1e-10
    
    def test_data_store_entries(self, data_structures):
        """Test DataStore with HashMap field."""
        store = data_structures.DataStore("entries_test")
        
        store.add_entry("pi", 3.14159)
        store.add_entry("e", 2.71828)
        
        # entries should be accessible as dict
        assert store.entries["pi"] == 3.14159
        assert store.entries["e"] == 2.71828
    
    def test_counter(self, data_structures):
        """Test Counter struct."""
        counter = data_structures.Counter()
        
        assert counter.count == 0
        
        result = counter.increment(5)
        assert result == 5
        assert counter.count == 5
        
        counter.increment(10)
        assert counter.count == 15
        
        history = counter.get_history()
        assert 0 in history
        assert 5 in history
        assert 15 in history


class TestEnums:
    """Test enum wrapping."""
    
    def test_color_variants(self, data_structures):
        """Test C-like enum variants."""
        red = data_structures.Color.Red
        green = data_structures.Color.Green
        blue = data_structures.Color.Blue
        
        assert red is not None
        assert green is not None
        assert blue is not None
    
    def test_color_code(self, data_structures):
        """Test enum method."""
        red = data_structures.Color.Red
        assert red.code() == 0xFF0000
        
        blue = data_structures.Color.Blue
        assert blue.code() == 0x0000FF
    
    def test_op_status_variants(self, data_structures):
        """Test enum with data variants."""
        # OpStatus has data variants (Failed(String)) which are not fully
        # supported in v0.3.0 - skip this test for now
        if not hasattr(data_structures, 'OpStatus'):
            pytest.skip("OpStatus with data variants not yet supported")
        
        pending = data_structures.OpStatus.Pending
        completed = data_structures.OpStatus.Completed
        
        assert pending.label() == "Pending"
        assert completed.label() == "Completed"
        
        assert not pending.is_terminal()
        assert completed.is_terminal()


class TestErrorHandling:
    """Test Result types and error propagation."""
    
    def test_validate_string_success(self, data_structures):
        """Test successful validation."""
        result = data_structures.validate_string("hello world", 100)
        assert result == "hello world"
    
    def test_validate_string_empty(self, data_structures):
        """Test EmptyInput error."""
        with pytest.raises(RuntimeError, match="EmptyInput"):
            data_structures.validate_string("", 100)
    
    def test_validate_string_too_long(self, data_structures):
        """Test TooLong error."""
        with pytest.raises(RuntimeError, match="TooLong"):
            data_structures.validate_string("a" * 100, 10)
    
    def test_validate_string_invalid_char(self, data_structures):
        """Test InvalidCharacter error."""
        with pytest.raises(RuntimeError, match="InvalidCharacter"):
            data_structures.validate_string("hello@world", 100)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
