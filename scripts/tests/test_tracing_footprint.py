"""Keep archive inspection portable and prevent rustdoc metadata from masquerading as code."""

import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location(
    "tracing_footprint", Path(__file__).resolve().parents[1] / "check-tracing-footprint.py")
GUARD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GUARD)


def member(name, content):
    header = (f"{name:<16}{0:<12}{0:<6}{0:<6}{0:<8}{len(content):<10}`\n").encode()
    assert len(header) == 60
    return header + content + (b"\n" if len(content) % 2 else b"")


class ArchiveTests(unittest.TestCase):
    def test_metadata_and_symbol_tables_do_not_count_as_logging_code(self):
        archive = b"!<arch>\n" + member("/", b"concord.call")
        archive += member("lib.rmeta/", b"rustdoc describes concord.call and log::Call")
        archive += member("core.o/", b"object")
        self.assertEqual(GUARD.object_code(archive), b"object")

    def test_gnu_long_object_names_and_odd_member_sizes(self):
        archive = b"!<arch>\n" + member("//", b"core.long.rcgu.o/\n")
        archive += member("/0", b"one") + member("second.obj/", b"two")
        self.assertEqual(GUARD.object_code(archive), b"onetwo")

    def test_bsd_extended_names(self):
        name = b"core.long.rcgu.o"
        archive = b"!<arch>\n" + member(f"#1/{len(name)}", name + b"code")
        self.assertEqual(GUARD.object_code(archive), b"code")

    def test_missing_object_code_fails_closed(self):
        with self.assertRaisesRegex(AssertionError, "no object code"):
            GUARD.object_code(b"!<arch>\n" + member("lib.rmeta/", b"metadata"))


if __name__ == "__main__":
    unittest.main()
