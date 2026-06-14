"""
End-to-end integration tests for string-utils fixture.

Tests string handling, &str conversions, and UTF-8 operations.
"""

import pytest
import sys
from pathlib import Path

fixture_dir = Path(__file__).parent.parent / "fixtures"
sys.path.insert(0, str(fixture_dir))


@pytest.fixture(scope="module")
def string_utils():
    """Import the wrapped string-utils package."""
    try:
        import py_string_utils as pkg
        return pkg
    except ImportError as e:
        pytest.skip(f"py_string_utils not built yet: {e}")


class TestStringFunctions:
    """Test string manipulation functions."""
    
    def test_reverse(self, string_utils):
        """Test string reversal."""
        result = string_utils.reverse("hello")
        assert result == "olleh"
        
        result = string_utils.reverse("")
        assert result == ""
        
        # Test Unicode
        result = string_utils.reverse("hello 🦀")
        assert "🦀" in result
    
    def test_word_count(self, string_utils):
        """Test word counting."""
        result = string_utils.word_count("hello world")
        assert result == 2
        
        result = string_utils.word_count("")
        assert result == 0
        
        result = string_utils.word_count("one")
        assert result == 1
    
    def test_is_palindrome(self, string_utils):
        """Test palindrome detection."""
        assert string_utils.is_palindrome("racecar") == True
        assert string_utils.is_palindrome("hello") == False
        assert string_utils.is_palindrome("A man a plan a canal Panama") == True
        assert string_utils.is_palindrome("") == True
    
    def test_truncate(self, string_utils):
        """Test string truncation."""
        result = string_utils.truncate("hello world", 20)
        assert result == "hello world"  # Not truncated
        
        result = string_utils.truncate("hello world", 5)
        assert result == "he..."  # Truncated with ellipsis
        
        result = string_utils.truncate("hi", 10)
        assert result == "hi"


class TestTextProcessor:
    """Test TextProcessor struct with String fields."""
    
    def test_text_processor_creation(self, string_utils):
        """Test TextProcessor construction."""
        processor = string_utils.TextProcessor(">> ", False)
        assert processor.prefix == ">> "
        assert processor.uppercase == False
    
    def test_text_processor_process(self, string_utils):
        """Test process method."""
        processor = string_utils.TextProcessor("PREFIX: ", False)
        result = processor.process("hello")
        assert result == "PREFIX: hello"
        
        # Test with uppercase enabled
        processor_upper = string_utils.TextProcessor("[", True)
        result = processor_upper.process("hello")
        assert result == "[HELLO"
    
    def test_text_processor_greet(self, string_utils):
        """Test greet method."""
        processor = string_utils.TextProcessor("Hello, ", False)
        result = processor.greet("World")
        assert result == "Hello, World"


class TestProcessStatus:
    """Test ProcessStatus enum."""
    
    def test_status_variants(self, string_utils):
        """Test enum variants."""
        success = string_utils.ProcessStatus.Success
        empty = string_utils.ProcessStatus.EmptyInput
        too_long = string_utils.ProcessStatus.TooLong
        
        assert success is not None
        assert empty is not None
        assert too_long is not None
    
    def test_status_is_ok(self, string_utils):
        """Test is_ok method."""
        success = string_utils.ProcessStatus.Success
        assert success.is_ok() == True
        
        empty = string_utils.ProcessStatus.EmptyInput
        assert empty.is_ok() == False
    
    def test_status_description(self, string_utils):
        """Test description method."""
        success = string_utils.ProcessStatus.Success
        desc = success.description()
        assert "success" in desc.lower()
        
        empty = string_utils.ProcessStatus.EmptyInput
        desc = empty.description()
        assert "empty" in desc.lower()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
