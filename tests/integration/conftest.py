"""
Pytest configuration for integration tests.

Handles building the fixtures before running tests.
"""

import subprocess
import sys
from pathlib import Path
import pytest


def pytest_configure(config):
    """
    Hook called before test collection.
    
    Attempts to build all fixtures if VERTUMNUS_SKIP_BUILD is not set.
    """
    if not config.getoption("--collect-only"):
        skip_build = config.getoption("--skip-build", default=False)
        
        if not skip_build:
            print("\n=== Building test fixtures with vertumnus ===")
            build_fixtures()


def pytest_addoption(parser):
    """Add custom command-line options."""
    parser.addoption(
        "--skip-build",
        action="store_true",
        default=False,
        help="Skip building fixtures (assume they're already built)"
    )


def build_fixtures():
    """
    Build all test fixtures using vertumnus wrap.
    
    Runs vertumnus on each fixture crate to generate Python bindings.
    """
    fixtures_dir = Path(__file__).parent.parent / "fixtures"
    fixtures = ["simple-math", "data-structures", "string-utils"]
    
    for fixture in fixtures:
        fixture_path = fixtures_dir / fixture
        if not fixture_path.exists():
            print(f"⚠️  Fixture not found: {fixture_path}")
            continue
        
        print(f"Building {fixture}...")
        
        try:
            # Run vertumnus wrap on the fixture
            result = subprocess.run(
                ["vertumnus", "wrap", str(fixture_path), "--overwrite"],
                capture_output=True,
                text=True,
                timeout=120
            )
            
            if result.returncode == 0:
                print(f"✅ Built {fixture}")
            else:
                print(f"⚠️  Failed to build {fixture}:")
                print(result.stderr)
        
        except FileNotFoundError:
            print("⚠️  vertumnus not found in PATH. Install with: cargo install vertumnus-cli")
            print("    Skipping fixture builds.")
            break
        
        except subprocess.TimeoutExpired:
            print(f"⚠️  Timeout building {fixture}")
        
        except Exception as e:
            print(f"⚠️  Error building {fixture}: {e}")


@pytest.fixture(scope="session")
def fixtures_dir():
    """Return the path to the fixtures directory."""
    return Path(__file__).parent.parent / "fixtures"
