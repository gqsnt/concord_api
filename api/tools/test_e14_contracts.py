"""Public extension and documentation regressions for the follow-up reading."""
from pathlib import Path
import unittest
API = Path(__file__).resolve().parents[1]
class FollowupContracts(unittest.TestCase):
    def test_n2_bounded_has_no_unused_depth_accessor(self):
        text = (API / "concord-core/src/body.rs").read_text()
        self.assertNotIn("pub fn has_entry_depth_error", text)
    def test_r3_readers_and_pagers_use_the_public_helpers(self):
        for name in ("json_decode.rs", "page.rs"):
            text = (API / "concord-extras/src" / name).read_text()
            self.assertNotIn("__private::cut_details", text)
            self.assertNotIn("__private::without_fragment", text)
        self.assertIn("pub fn details(", (API / "concord-core/src/read.rs").read_text())
        self.assertIn("pub fn fragmentless(", (API / "concord-core/src/page.rs").read_text())
    def test_r1_measure_of_record_uses_the_named_macro_shape(self):
        text = (API / "concord/benches/macro_get.rs").read_text()
        self.assertIn("concord::api!", text)
        self.assertIn("query { start; count; queue; }", text)
        self.assertIn('path "by-riot-id/{game_name}/{tag_line}"', text)
        self.assertIn("tour_gen::Shapes::new()", text)
