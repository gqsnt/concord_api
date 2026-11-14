"""Mutation-runner infrastructure regressions; no Cargo commands are launched."""

from contextlib import redirect_stdout
import io
import fcntl
import json
import os
from pathlib import Path
from types import SimpleNamespace
import sys
import select
import subprocess
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import mutate
import matrix
from links import anchors


class RunnerTests(unittest.TestCase):
    def setUp(self):
        target = mutate.REPO_ROOT / "target"
        target.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=target, prefix="mutation-runner-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "source/concord-core/src").mkdir(parents=True)
        (self.root / "source/concord-core/src/lib.rs").write_text("unmutated source")
        (self.root / "source/docs/reference").mkdir(parents=True)
        (self.root / "source/docs/reference/messages.md").write_text("macro catalogue fixture")
        (self.root / "rustlib").mkdir()

    def args(self, sockets=False):
        return SimpleNamespace(packages=["all"], exclude_test_target=[], skip_test=[],
                               build_timeout=10, timeout=10, with_sockets=sockets)

    def invoke(self, command, cwd, env, log, timeout):
        if command[0] == "cargo":
            packages = [{"name": "concord-core", "id": "core", "targets": []},
                        {"name": "concord-testkit", "id": "kit", "targets": [
                            {"name": "server", "kind": ["test"]}]}]
            log.write_text(json.dumps({"workspace_members": ["core", "kit"], "packages": packages}) + "\n")
        else:
            log.write_text(str(self.root / "rustlib") + "\n")
        return 0, False

    def test_a_killed_runs_mutated_build_is_never_reused(self):
        poison = self.root / "worker-1/target/debug/mutated-binary"
        poison.parent.mkdir(parents=True)
        poison.write_text("compiled mutated source, misleading old mtime")
        with patch.object(mutate, "ROOT", self.root), patch.object(mutate, "invoke", self.invoke):
            worker = mutate.Worker(1, self.args())
        self.assertFalse(poison.exists())
        self.assertEqual((worker.source / "concord-core/src/lib.rs").read_text(), "unmutated source")
        self.assertEqual((worker.source / "docs/reference/messages.md").read_text(), "macro catalogue fixture")
        self.assertIn(str(self.root / "rustlib"), worker.env["LD_LIBRARY_PATH"])
        self.assertEqual(worker.env["CARGO_BUILD_JOBS"], "4")

    def test_socket_target_is_excluded_only_without_the_explicit_flag(self):
        for sockets in (False, True):
            args = self.args(sockets)
            with patch.object(mutate, "ROOT", self.root), patch.object(mutate, "invoke", self.invoke):
                mutate.Worker(1, args)
            self.assertEqual("concord-testkit::server" in args.exclude_test_target, not sockets)

    def test_root_snapshot_keeps_nested_members_and_docs_without_build_or_git_data(self):
        source = self.root / "source"
        for name in ("Cargo.toml", "tests/tour/src/lib.rs", "docs/reference/contracts.md",
                     "target/debug/output", ".git/config", "tests/tour/target/output"):
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name)
        destination = self.root / "snapshot"
        mutate.copy_sources(source, destination)
        for name in ("Cargo.toml", "tests/tour/src/lib.rs", "docs/reference/contracts.md"):
            self.assertEqual((destination / name).read_text(), name)
        for name in ("target", ".git", "tests/tour/target"):
            self.assertFalse((destination / name).exists())

    def test_cargo_and_test_children_keep_the_invocation_lock_after_parent_death(self):
        ready, release, done = [self.root / name for name in ("ready", "release", "done")]
        for path in (ready, release, done):
            os.mkfifo(path)
        ready_fd = os.open(ready, os.O_RDWR | os.O_NONBLOCK)
        done_fd = os.open(done, os.O_RDWR | os.O_NONBLOCK)
        child = """
import os, sys
release = os.open(sys.argv[3], os.O_RDWR)
with open(sys.argv[2], 'w') as ready:
    ready.write(str(os.getpid()))
os.read(release, 1)
os.close(int(sys.argv[1]))
with open(sys.argv[4], 'w') as done:
    done.write('released')
"""
        launcher = """
import fcntl, os, pathlib, sys
sys.path.insert(0, sys.argv[1])
import mutate
root = pathlib.Path(sys.argv[2])
with (root / 'lock').open('a') as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    mutate.RUN_LOCK_FD = lock.fileno()
    mutate.invoke([sys.executable, '-c', sys.argv[3], str(lock.fileno()),
                   str(root / 'ready'), str(root / 'release'), str(root / 'done')],
                  root, os.environ.copy(), root / 'child.log', 10)
"""
        parent = subprocess.Popen([sys.executable, "-B", "-c", launcher,
                                   str(Path(mutate.__file__).parent), str(self.root), child],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        child_pid = None
        released = False
        try:
            self.assertTrue(select.select([ready_fd], [], [], 10)[0], "child did not become ready")
            child_pid = int(os.read(ready_fd, 64))
            parent.kill()  # SIGKILL bypasses the runner's Python cleanup.
            parent.wait(timeout=10)
            with (self.root / "lock").open("a") as contender:
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with release.open("wb") as unblock:
                    unblock.write(b"x")
                self.assertTrue(select.select([done_fd], [], [], 10)[0], "child did not release its lock")
                released = os.read(done_fd, 64) == b"released"
                self.assertTrue(released)
                fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
        finally:
            if parent.poll() is None:
                parent.kill()
            parent.communicate(timeout=10)
            if child_pid is not None and not released:
                try:
                    os.kill(child_pid, 9)
                except ProcessLookupError:
                    pass
            os.close(ready_fd)
            os.close(done_fd)

    def test_catalogue_anchors_and_socket_contract_are_current(self):
        catalogue = mutate.catalog()
        for item in catalogue:
            source = (mutate.REPO_ROOT / item["file"]).read_text()
            self.assertEqual(source.count(item["find"]), 1, item["id"])
        by_id = {item["id"]: item for item in catalogue}
        self.assertTrue(by_id["M46"]["requires_sockets"])
        self.assertIn("C2.16", by_id["M92"]["contract"])
        self.assertNotEqual(mutate.DEFAULT_PACKAGES, ("all",))

    def test_listing_needs_no_baseline_build(self):
        with patch.object(sys, "argv", ["mutate.py", "--list"]), redirect_stdout(io.StringIO()), patch.object(mutate, "run") as run:
            self.assertEqual(mutate.main(), 0)
            run.assert_not_called()

    def test_contract_references_name_current_contracts_or_page_headings(self):
        root = mutate.REPO_ROOT
        known = {item.number for item in matrix.read_contracts(root / "docs/reference/contracts.md")}
        for item in mutate.catalog():
            reference = item["contract"]
            with self.subTest(mutation=item["id"], reference=reference):
                if "#" in reference:
                    page, heading = reference.split("#", 1)
                    self.assertIn(heading, anchors(root / page))
                else:
                    self.assertRegex(reference, r"^C\d+\.\d+(?:, C\d+\.\d+)*$")
                    self.assertLessEqual(set(reference.split(", ")), known)


if __name__ == "__main__":
    unittest.main()
