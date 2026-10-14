"""Headless Blender unittest runner.

Usage: blender -b --factory-startup --python-exit-code 1 -P scripts/blender/run_tests.py -- [module]
Raises on failure so --python-exit-code turns it into a non-zero exit (Blender exits 0 otherwise).
"""
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))  # `lib` and `tests` import as packages
argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
loader = unittest.TestLoader()
if argv and argv[0]:
    suite = loader.loadTestsFromName(f"tests.{argv[0]}")
else:
    suite = loader.discover(str(ROOT / "tests"), pattern="test_*.py", top_level_dir=str(ROOT))
result = unittest.TextTestRunner(verbosity=2).run(suite)
if result.testsRun == 0:
    raise RuntimeError("blender tests: no tests ran (bad module name or discovery path)")
if not result.wasSuccessful():
    raise RuntimeError(f"blender tests failed: {len(result.failures)} failures, {len(result.errors)} errors")
