"""Checks for evidence attribution parsers and credential-safe redirects."""
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.request

from qualification_attribution import parse_layouts, parse_passes, replace_once
from qualification_ci_logs import _SafeRedirect, _request, github_bytes, excerpt, inspect_native_zip, validate_run


class AttributionTests(unittest.TestCase):
    def test_layout_fields_remain_attached_to_their_native_type(self):
        output = (
            "print-type-size type: " + chr(96) + "Input" + chr(96) + ": 24 bytes, alignment: 8 bytes\n"
            "print-type-size     field " + chr(96) + ".body" + chr(96) + ": 16 bytes\n"
            "ordinary compiler line\n"
            "print-type-size type: " + chr(96) + "Future" + chr(96) + ": 40 bytes, alignment: 8 bytes\n"
        )
        value = parse_layouts(output)
        self.assertEqual([p["bytes"] for p in value], [24, 40])
        self.assertEqual(len(value[0]["fields"]), 1)
        self.assertEqual(value[1]["fields"], [])
        with self.assertRaises(ValueError):
            parse_layouts("no compiler layout output")

    def test_phase_timings_preserve_native_line_without_summing(self):
        line = "time:   0.250; rss: 100MB -> 120MB (20MB); type_check_crate"
        value = parse_passes(line + "\ntime: 0.500; rss: 120MB; total")
        self.assertEqual(value[0]["seconds"], 0.25)
        self.assertEqual(value[0]["native_line"], line)
        self.assertEqual(value[1]["phase"], "total")
        with self.assertRaises(ValueError):
            parse_passes("time: 0.500; rss: 120MB; total")

    def test_native_rustc_tab_separates_rss_from_phase(self):
        line = "time:   5.727; rss:  750MB ->  987MB ( +237MB)\ttype_check_crate"
        total = "time:  15.367; rss:   71MB ->  155MB (  +84MB)\ttotal"
        value = parse_passes(line + "\n" + total)
        self.assertEqual([row["phase"] for row in value], ["type_check_crate", "total"])
        self.assertEqual([row["seconds"] for row in value], [5.727, 15.367])
        self.assertEqual(value[0]["native_line"], line)

    def test_patch_requires_exactly_one_source_anchor(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.rs"
            path.write_text("old old")
            with self.assertRaises(RuntimeError):
                replace_once(path, "old", "new")
            self.assertEqual(path.read_text(), "old old")

    def test_cross_origin_redirect_strips_credentials(self):
        source = urllib.request.Request(
            "https://api.github.com/repos/gqsnt/concord_api/actions/1",
            headers={"Authorization": "Bearer sentinel", "Cookie": "sentinel", "Proxy-Authorization": "sentinel"},
        )
        result = _SafeRedirect().redirect_request(source, None, 302, "redirect", {}, "https://example.test/bytes")
        self.assertFalse(any(k.lower() in {"authorization", "cookie", "proxy-authorization"} for k, _ in result.header_items()))
        same = _SafeRedirect().redirect_request(source, None, 302, "redirect", {}, "https://api.github.com/path")
        self.assertEqual(same.get_header("Authorization"), "Bearer sentinel")
        with self.assertRaises(Exception):
            _SafeRedirect().redirect_request(source, None, 302, "redirect", {}, "http://example.test/bytes")

    def test_repository_path_validation_precedes_token_lookup(self):
        for path in ["https://example.test", "../other", "/repos/other", "actions/../other"]:
            with self.assertRaises(RuntimeError):
                _request(path)

    def test_incomplete_download_never_replaces_existing_file(self):
        class Response(io.BytesIO):
            status = 200
            headers = {"Content-Length": "8"}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "archive.zip"
            path.write_bytes(b"prior")
            with patch("qualification_ci_logs._request", side_effect=lambda _: Response(b"short")), patch("qualification_ci_logs.time.sleep"):
                with self.assertRaises(RuntimeError):
                    github_bytes("actions/example", path)
            self.assertEqual(path.read_bytes(), b"prior")
            self.assertEqual(list(Path(directory).glob("*.part")), [])


    def test_derived_excerpt_redacts_credentials_and_retains_failure_text(self):
        with patch.dict("os.environ", {"GH_TOKEN": "test-secret-token"}):
            result = excerpt(io.StringIO("error: https://example.test/signed?key=secret test-secret-token FAILED\n"))
        rendered = str(result)
        self.assertNotIn("test-secret-token", rendered)
        self.assertNotIn("key=secret", rendered)
        self.assertIn("FAILED", rendered)
        self.assertIn("Derived text candidates only", result["warning"])

    def test_run_identity_rejects_a_different_source(self):
        value = {
            "id": 1, "run_number": 2, "head_sha": "a" * 40, "name": "CI",
            "event": "push", "head_branch": "concord-v3",
            "repository": {"full_name": "gqsnt/concord_api"},
        }
        validate_run(value, "CI", 2, 1, "a" * 40)
        with self.assertRaises(RuntimeError):
            validate_run(value, "CI", 2, 1, "b" * 40)

    def test_native_archive_crc_is_checked_without_path_extraction(self):
        import zipfile
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "native.zip"
            with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as archive:
                archive.writestr("../not-extracted.txt", "native terminal marker")
            value = inspect_native_zip(path)
            self.assertTrue(value["members"][0]["crc_verified"])
            self.assertFalse((Path(directory).parent / "not-extracted.txt").exists())
            raw = bytearray(path.read_bytes())
            position = raw.index(b"native terminal marker")
            raw[position] ^= 1
            path.write_bytes(raw)
            with self.assertRaises(zipfile.BadZipFile):
                inspect_native_zip(path)


if __name__ == "__main__":
    unittest.main()
