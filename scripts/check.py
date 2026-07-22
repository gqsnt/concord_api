#!/usr/bin/env python3
"""Validate Concord's documentation and lightweight repository structure.

The checker uses only the Python standard library. It intentionally checks the
manual, examples, public export coverage, Cargo paths, and standalone fixture
purpose without trying to replace Rust compilation or tests.
"""

from __future__ import annotations

import os
import re
import sys
import tomllib
from collections import Counter
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
SUMMARY = DOCS / "SUMMARY.md"

LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
TOP_HEADING_RE = re.compile(r"^#\s+\S", re.MULTILINE)
PUB_USE_RE = re.compile(r"pub\s+use\s+.*?;", re.DOTALL)

FORBIDDEN_DOC_PATTERNS = {
    "removed StreamResponse type": re.compile(r"\bStreamResponse(?:<[^>]+>)?\b"),
    "removed pagination patch API": re.compile(r"\bPaginationRequest\b|\bPaginationPatch\b"),
    "removed rate-limit template block": re.compile(r"\brate_limits\s*\{"),
    "removed generated rate-limit dimensions": re.compile(r"\bby\s*\["),
    "removed rate-limit replace": re.compile(r"\brate_limit\s+replace\b"),
    "removed rate-limit off": re.compile(r"\brate_limit\s+off\b"),
    "internal phase language": re.compile(r"\bPhase\s+\d+\b", re.IGNORECASE),
    "internal evidence language": re.compile(
        r"\bevidence rows?\b|\bclaim registry\b|\bcontroller state\b|\bevidence anchor\b",
        re.IGNORECASE,
    ),
    "removed task file": re.compile(r"\btodo\.md\b", re.IGNORECASE),
    "distribution roadmap language": re.compile(
        r"\bcrates\.io\b|\bdocs\.rs\b|\bpublish\s*=\s*false\b|\bpublication\b|\blicen[cs]e\b",
        re.IGNORECASE,
    ),
    "unfinished documentation marker": re.compile(r"\bTODO\b|\bTBD\b|\bcoming soon\b", re.IGNORECASE),
}

MERMAID_STARTS = (
    "flowchart",
    "graph",
    "sequenceDiagram",
    "stateDiagram",
    "classDiagram",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
    "quadrantChart",
    "gitGraph",
    "C4",
)


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def markdown_files() -> list[Path]:
    return [ROOT / "README.md", *sorted(DOCS.rglob("*.md"))]


def strip_fenced_blocks(text: str) -> str:
    out: list[str] = []
    in_fence = False
    for line in text.splitlines():
        if line.startswith("```"):
            in_fence = not in_fence
            out.append("")
        elif in_fence:
            out.append("")
        else:
            out.append(line)
    return "\n".join(out)


def github_slug(text: str) -> str:
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"[`*_~]", "", text).strip().lower()
    chars: list[str] = []
    for char in text:
        if char.isalnum() or char in {"-", "_", " "}:
            chars.append(char)
    slug = "".join(chars).replace(" ", "-")
    return re.sub(r"-+", "-", slug).strip("-")


def anchors(path: Path) -> set[str]:
    if path.suffix.lower() != ".md":
        return set()
    counts: Counter[str] = Counter()
    result: set[str] = set()
    in_fence = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        match = re.match(r"^#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        base = github_slug(match.group(1))
        if not base:
            continue
        number = counts[base]
        counts[base] += 1
        result.add(base if number == 0 else f"{base}-{number}")
    return result


def clean_link_target(raw: str) -> tuple[str, str]:
    target = raw.strip()
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    if " " in target and not target.startswith(("http://", "https://")):
        target = target.split(" ", 1)[0]
    target = unquote(target)
    path, _, anchor = target.partition("#")
    return path, anchor


def resolve_link(source: Path, raw: str) -> tuple[Path | None, str]:
    path_part, anchor = clean_link_target(raw)
    if raw.startswith(("http://", "https://", "mailto:")):
        return None, anchor
    if not path_part:
        return source, anchor
    candidate = (ROOT / path_part.lstrip("/")) if path_part.startswith("/") else (source.parent / path_part)
    return candidate.resolve(), anchor


def check_links(errors: list[str]) -> None:
    anchor_cache: dict[Path, set[str]] = {}
    for source in markdown_files():
        text = source.read_text(encoding="utf-8")
        for match in LINK_RE.finditer(text):
            raw = match.group(1)
            target, anchor = resolve_link(source, raw)
            if target is None:
                continue
            try:
                target.relative_to(ROOT)
            except ValueError:
                errors.append(f"{relative(source)}: link escapes repository: {raw}")
                continue
            if not target.exists():
                line = text.count("\n", 0, match.start()) + 1
                errors.append(f"{relative(source)}:{line}: missing link target {raw}")
                continue
            if anchor:
                if target.is_dir():
                    errors.append(f"{relative(source)}: anchor targets a directory: {raw}")
                    continue
                if target.suffix.lower() != ".md":
                    continue
                available = anchor_cache.setdefault(target, anchors(target))
                if anchor not in available:
                    line = text.count("\n", 0, match.start()) + 1
                    errors.append(
                        f"{relative(source)}:{line}: missing anchor #{anchor} in {relative(target)}"
                    )


def check_layout(errors: list[str]) -> None:
    for path in markdown_files():
        if not TOP_HEADING_RE.search(path.read_text(encoding="utf-8")):
            errors.append(f"{relative(path)}: missing top-level title")

    for directory in sorted(path for path in DOCS.iterdir() if path.is_dir()):
        files = sorted(path for path in directory.iterdir() if path.is_file() and path.suffix == ".md")
        if not (directory / "README.md").exists():
            errors.append(f"{relative(directory)}: section has no README.md")
        if len(files) < 2:
            errors.append(f"{relative(directory)}: avoid a directory containing only README.md")
        nested = [path for path in directory.rglob("*") if path.is_dir()]
        if nested:
            errors.append(f"{relative(directory)}: nested page directories are not allowed: {relative(nested[0])}")

    if any(path.is_dir() for path in DOCS.rglob("*") if len(path.relative_to(DOCS).parts) > 1):
        errors.append("docs: documentation should remain at most one section directory deep")


def check_summary(errors: list[str]) -> None:
    if not SUMMARY.exists():
        errors.append("docs/SUMMARY.md is missing")
        return
    summary = SUMMARY.read_text(encoding="utf-8")
    linked: list[Path] = []
    for raw in LINK_RE.findall(summary):
        target, _ = resolve_link(SUMMARY, raw)
        if target is not None and target.suffix == ".md" and DOCS in target.parents:
            linked.append(target)
    counts = Counter(linked)
    for target, count in counts.items():
        if count != 1:
            errors.append(f"docs/SUMMARY.md: {relative(target)} appears {count} times")
    expected = {path.resolve() for path in DOCS.rglob("*.md") if path != SUMMARY}
    actual = set(linked)
    for path in sorted(expected - actual):
        errors.append(f"docs/SUMMARY.md: missing page {relative(path)}")
    for path in sorted(actual - expected):
        errors.append(f"docs/SUMMARY.md: unexpected page {relative(path)}")


def check_language(errors: list[str]) -> None:
    for path in markdown_files():
        text = strip_fenced_blocks(path.read_text(encoding="utf-8"))
        for label, pattern in FORBIDDEN_DOC_PATTERNS.items():
            match = pattern.search(text)
            if match:
                line = text.count("\n", 0, match.start()) + 1
                errors.append(f"{relative(path)}:{line}: {label}: {match.group(0)!r}")


def mermaid_blocks(text: str) -> list[tuple[int, list[str]]]:
    blocks: list[tuple[int, list[str]]] = []
    active = False
    start = 0
    lines: list[str] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if not active and line.strip() == "```mermaid":
            active = True
            start = number
            lines = []
        elif active and line.startswith("```"):
            blocks.append((start, lines))
            active = False
        elif active:
            lines.append(line)
    if active:
        blocks.append((start, []))
    return blocks


def check_fences_and_mermaid(errors: list[str]) -> None:
    for path in DOCS.rglob("*.md"):
        text = path.read_text(encoding="utf-8")
        in_fence = False
        for line in text.splitlines():
            if line.startswith("```"):
                in_fence = not in_fence
        if in_fence:
            errors.append(f"{relative(path)}: unterminated fenced code block")
        for start, lines in mermaid_blocks(text):
            first = next((line.strip() for line in lines if line.strip()), "")
            if not first:
                errors.append(f"{relative(path)}:{start}: empty or unterminated Mermaid block")
            elif not first.startswith(MERMAID_STARTS):
                errors.append(f"{relative(path)}:{start}: unsupported Mermaid diagram start {first!r}")


def load_toml(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        raise ValueError(f"{relative(path)}: invalid TOML: {exc}") from exc


def check_features(errors: list[str]) -> None:
    try:
        manifest = load_toml(ROOT / "concord_core" / "Cargo.toml")
    except ValueError as exc:
        errors.append(str(exc))
        return
    features = set(manifest.get("features", {})) - {"default"}
    combined = "\n".join(path.read_text(encoding="utf-8") for path in DOCS.rglob("*.md"))
    for feature in sorted(features):
        if f"`{feature}`" not in combined:
            errors.append(f"feature `{feature}` is not documented")


def check_examples(errors: list[str]) -> None:
    docs_text = "\n".join(path.read_text(encoding="utf-8") for path in markdown_files())
    examples = sorted((ROOT / "concord_examples" / "examples").glob("*.rs"))
    for example in examples:
        if example.name not in docs_text:
            errors.append(f"{relative(example)}: runnable example is not linked from the manual")


def exported_symbols(path: Path) -> set[str]:
    text = path.read_text(encoding="utf-8")
    result: set[str] = set()
    for statement in PUB_USE_RE.findall(text):
        aliases = re.findall(r"\bas\s+([A-Z][A-Za-z0-9_]*)", statement)
        if aliases:
            result.update(aliases)
            continue
        result.update(re.findall(r"\b[A-Z][A-Za-z0-9_]*\b", statement))
    return result - {"Self"}


def check_public_exports(errors: list[str]) -> None:
    api = (DOCS / "reference" / "api.md").read_text(encoding="utf-8")
    for source in [
        ROOT / "concord_core" / "src" / "prelude.rs",
        ROOT / "concord_core" / "src" / "advanced.rs",
        ROOT / "concord_core" / "src" / "dangerous.rs",
    ]:
        for symbol in sorted(exported_symbols(source)):
            if f"`{symbol}`" not in api:
                errors.append(f"docs/reference/api.md: public export `{symbol}` from {relative(source)} is missing")


def iter_path_dependencies(value):
    if isinstance(value, dict):
        if isinstance(value.get("path"), str):
            yield value["path"]
        for nested in value.values():
            yield from iter_path_dependencies(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from iter_path_dependencies(nested)


def check_manifests(errors: list[str]) -> None:
    manifests = sorted(ROOT.rglob("Cargo.toml"))
    for manifest_path in manifests:
        try:
            manifest = load_toml(manifest_path)
        except ValueError as exc:
            errors.append(str(exc))
            continue
        for raw in iter_path_dependencies(manifest):
            target = (manifest_path.parent / raw).resolve()
            if not target.exists():
                errors.append(f"{relative(manifest_path)}: missing path dependency {raw}")

    try:
        workspace = load_toml(ROOT / "Cargo.toml").get("workspace", {})
    except ValueError:
        return
    for member in workspace.get("members", []):
        if not (ROOT / member / "Cargo.toml").exists():
            errors.append(f"Cargo.toml: workspace member `{member}` is missing")


def check_fixtures(errors: list[str]) -> None:
    fixture_workspace = ROOT / "concord_macros" / "tests" / "fixtures"
    manifest_path = fixture_workspace / "Cargo.toml"
    manifest = load_toml(manifest_path)
    members = manifest.get("workspace", {}).get("members", [])
    compile_source = (ROOT / "concord_macros" / "tests" / "compile.rs").read_text(encoding="utf-8")
    fixture_readme = (fixture_workspace / "README.md").read_text(encoding="utf-8")
    for member in members:
        package = fixture_workspace / member
        if not (package / "Cargo.toml").exists():
            errors.append(f"{relative(package)}: fixture has no Cargo.toml")
            continue
        rust_files = list(package.rglob("*.rs"))
        if not rust_files:
            errors.append(f"{relative(package)}: empty fixture package")
        key = member.split("/")[0]
        if key not in compile_source:
            errors.append(f"{relative(package)}: fixture is not referenced by concord_macros/tests/compile.rs")
        if f"`{key}`" not in fixture_readme:
            errors.append(f"{relative(package)}: fixture purpose is not documented")

    core_fixture_checks = {
        "concord_core/tests/fixtures/config_features": "concord_core/tests/config_methods.rs",
        "concord_core/tests/fixtures/development_features": "concord_core/tests/development.rs",
    }
    for fixture, owner in core_fixture_checks.items():
        fixture_path = ROOT / fixture
        if not fixture_path.exists():
            errors.append(f"{fixture}: missing fixture directory")
        if fixture_path.name not in (ROOT / owner).read_text(encoding="utf-8"):
            errors.append(f"{owner}: does not reference {fixture_path.name}")

    for obsolete in ["equal_name", "wip"]:
        for path in ROOT.rglob(obsolete):
            if path.is_dir() and "target" not in path.parts:
                errors.append(f"{relative(path)}: obsolete or placeholder test directory remains")


def check_empty_directories(errors: list[str]) -> None:
    ignored = {"target", ".git"}
    for directory in ROOT.rglob("*"):
        if not directory.is_dir() or any(part in ignored for part in directory.parts):
            continue
        try:
            next(directory.iterdir())
        except StopIteration:
            errors.append(f"{relative(directory)}: empty directory")


def main() -> int:
    errors: list[str] = []
    for check in [
        check_links,
        check_layout,
        check_summary,
        check_language,
        check_fences_and_mermaid,
        check_features,
        check_examples,
        check_public_exports,
        check_manifests,
        check_fixtures,
        check_empty_directories,
    ]:
        try:
            check(errors)
        except Exception as exc:  # Keep all structural failures visible in one run.
            errors.append(f"{check.__name__}: unexpected error: {exc}")

    if errors:
        print(f"repository check failed with {len(errors)} issue(s):", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    pages = len(list(DOCS.rglob("*.md")))
    examples = len(list((ROOT / "concord_examples" / "examples").glob("*.rs")))
    print(
        f"repository check passed: {pages} documentation pages, "
        f"{examples} runnable examples, links/anchors/features/exports/fixtures valid"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
