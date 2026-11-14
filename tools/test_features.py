"""Detached feature-check manifest regressions."""
import unittest
from features import detached_manifest, feature_sets, FORWARDED


class Isolation(unittest.TestCase):
    def test_concord_combinations_and_each_forwarder_are_checked_without_duplicates(self):
        own = {"json", "form", "multipart", "tower"}
        sets = feature_sets(own | FORWARDED | {"default"})
        self.assertEqual(len(sets), 39)
        self.assertEqual(len(sets), len(set(sets)))
        self.assertNotIn("default", {name for subset in sets for name in subset})
        for subset in sets:
            local = set(subset) & own
            forwarded = set(subset) & FORWARDED
            self.assertTrue(not forwarded or forwarded == FORWARDED
                            or (not local and len(forwarded) == 1))
        for bits in range(16):
            selected = {name for i, name in enumerate(sorted(own)) if bits & (1 << i)}
            self.assertIn(tuple(sorted(selected)), sets)
            self.assertIn(tuple(sorted(selected | FORWARDED)), sets)
        for name in FORWARDED:
            self.assertIn((name,), sets)

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
