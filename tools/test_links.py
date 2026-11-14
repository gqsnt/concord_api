"""Check Markdown discovery, heading slugs, and the anchor warning policy."""

from contextlib import redirect_stdout
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import links


class Links(unittest.TestCase):
    def setUp(self):
        target = links.ROOT / "target"
        target.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=target)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def page(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def test_github_headings_include_duplicates_and_exclude_code(self):
        page = self.page("page.md", """# Hello, `API`!
## Hello, API!
### Hello, API!-1
  ## [A link](page.md) and **bold** ###
Setext title
============
```rust
# hidden
```
~~~
# also hidden
~~~
## Unicode café
""")
        self.assertEqual(links.anchors(page), {
            "hello-api", "hello-api-1", "hello-api-1-1", "a-link-and-bold",
            "setext-title", "unicode-café",
        })

    def test_reference_links_angle_destinations_and_fences(self):
        page = self.page("page.md", """[Inline](<a file.md#some-heading> "Title")
[Reference][target] and [target]
[target]: other.md#heading
`[code](missing.md)`
```
[fenced](missing.md)
```
""")
        self.assertEqual(list(links.links(page)), [
            (1, "a file.md#some-heading"), (2, "other.md#heading"),
            (2, "other.md#heading"),
        ])

    def test_every_tracked_markdown_page_is_checked_with_separate_counts(self):
        self.page("crate/README.md", "[ok](../page.md#heading) [bad](../page.md#gone)\n")
        self.page("page.md", "# Heading\n")
        self.page("untracked.md", "[bad](missing.md)\n")
        with patch.object(links, "ROOT", self.root), patch.object(
            links.subprocess, "check_output", return_value=b"crate/README.md\0page.md\0Cargo.toml\0"
        ) as tracked, redirect_stdout(io.StringIO()) as output:
            self.assertEqual(links.main(["--anchors=warn"]), 0)
            self.assertEqual(links.main([]), 1)
            self.assertIn("0 broken files, 1 broken anchors", output.getvalue())
            self.assertEqual(tracked.call_args.kwargs["cwd"], self.root)
            self.page("crate/README.md", "[bad](missing.md)\n")
            self.assertEqual(links.main(["--anchors=warn"]), 1)
            self.assertIn("1 broken files, 0 broken anchors", output.getvalue())


if __name__ == "__main__":
    unittest.main()
