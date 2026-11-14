"""Detached feature-check manifest regressions."""
import unittest
from features import detached_manifest


class Isolation(unittest.TestCase):
    def test_inheritance_is_expanded_without_adding_dependency_features(self):
        source = '''[package]
name = "fixture"
edition.workspace = true
[dependencies]
serde.workspace = true
tokio = { workspace = true, features = ["net"], optional = true }
[lints]
workspace = true
'''
        workspace = {"package": {"edition": "2021"}, "dependencies": {"serde": {"version": "1", "features": ["derive"]}, "tokio": {"version": "1", "features": ["rt"]}}, "lints": {"rust": {"missing_docs": "deny"}}}
        result = detached_manifest(source, workspace)
        self.assertIn('serde = { version = "1", features = ["derive"] }', result)
        self.assertIn('[lints.rust]\nmissing_docs = "deny"', result)
        self.assertIn('[workspace]\nresolver = "2"', result)
        self.assertNotIn("workspace = true", result)
        self.assertIn('features = ["rt", "net"], optional = true', result)


if __name__ == "__main__":
    unittest.main()
