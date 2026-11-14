"""Release surface/documentation regressions for the public API."""
from pathlib import Path
import re
import tomllib
import unittest
REPO_ROOT = Path(__file__).resolve().parents[1]
class ReleaseContracts(unittest.TestCase):
    def test_helper_has_the_whole_shared_call_surface(self):
        text = (REPO_ROOT / "concord-core/src/__private/call.rs").read_text().split("#[cfg(test)]")[0]
        for method in ("send", "send_full", "send_raw", "send_if_none_match", "with_retry", "with_timeout", "with_idle_timeout", "request", "into_request", "edit_request", "read_response", "__prepare_if_none_match", "answered_from", "at_url", "into_future"):
            self.assertIn("fn " + method + "(", text)
        self.assertIn("$($pages)*", text)
    def test_body_writers_use_one_public_documented_door(self):
        text = (REPO_ROOT / "concord-core/src/body.rs").read_text()
        start = text.index("/// A bounded serialization")
        public = text.index("pub struct Bounded", start)
        self.assertNotIn("#[doc(hidden)]", text[start:public])
        self.assertIn("```", text[start:public])
    def test_extras_documents_its_hidden_response_url_helper(self):
        text = (REPO_ROOT / "concord-extras/src/lib.rs").read_text()
        self.assertIn("a hidden helper of the engine for the URL of an answer", text)
        self.assertNotIn("with nothing\n//! else of it", text)
    def test_c10_9_extras_pins_its_engine_release(self):
        manifest = tomllib.loads((REPO_ROOT / "concord-extras/Cargo.toml").read_text())
        version = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        self.assertEqual(manifest["dependencies"]["concord-core"].get("version"), "=" + version)
    def test_multipart_does_not_expose_getrandom(self):
        text = (REPO_ROOT / "concord-extras/src/multipart.rs").read_text()
        surface = text.split("pub enum MultipartError", 1)[1].split("impl fmt::Display", 1)[0]
        self.assertNotIn("getrandom::", surface)
    def test_release_has_no_stale_dead_code_exemptions(self):
        root = REPO_ROOT / "concord-core/src"
        for path in root.rglob("*.rs"):
            if path.name == "call.rs":
                continue  # The helper's placeholder names intentionally shield caller bindings.
            self.assertNotIn("#[allow(dead_code)]", path.read_text(), str(path))
        text = (root / "base.rs").read_text()
        self.assertRegex(text, r"#\[cfg\(test\)\]\s+pub\(crate\) fn join_path")
    def test_contract_names_identify_the_behavior_they_prove(self):
        for name in ("__private/call.rs", "__private/build.rs"):
            text = (REPO_ROOT / "concord-core/src" / name).read_text()
            self.assertNotRegex(text, r"fn c10_9_(?:call_methods|.*helper.*hygienic)|fn c2_5_literal_segments")
            self.assertNotRegex(text, r"fn c\d+_\d+_[ad]\d+_")
        body_tests = (REPO_ROOT / "concord-core/tests/prop_read.rs").read_text()
        self.assertNotRegex(body_tests, r"fn c\d+_\d+_[ad]\d+_")
        # Frame joining and announced-size allocation are whole-body contracts (C5.4).
        self.assertIn("fn c5_4_a_single_frame_is_returned_without_copying", body_tests)
        self.assertNotRegex(body_tests, r"fn c5_6_(?:a_single_frame|joining_reserves|an_unrepresentable_announced)")
    def test_reader_method_explains_the_incoming_and_error_contract(self):
        text = (REPO_ROOT / "concord-core/src/read.rs").read_text().split("pub trait Reader<T>", 1)[1].split("fn read", 1)[0]
        for word in ("Incoming", "limit", "ReadError", "clock"):
            self.assertIn(word, text)
