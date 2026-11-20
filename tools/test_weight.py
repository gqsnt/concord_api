from collections import Counter
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import weight


class DescriptionTests(unittest.TestCase):
    def test_full_mix_has_stable_prefix_and_input_shapes(self):
        mix = [weight.endpoint_shape(index) for index in range(30)]
        self.assertEqual(Counter(mix), {"get": 11, "json": 10, "status": 6, "pager": 3})
        self.assertEqual([i for i, shape in enumerate(mix) if shape == "pager"], [0, 10, 20])
        source = weight.description(100)
        self.assertEqual(len(re.findall(r"^\s+(?:GET|POST) endpoint_\d+\(", source, re.M)), 100)
        self.assertEqual(source.count("scope region_"), 1)
        self.assertEqual(source.count("scope group_"), 2)
        self.assertEqual(source.count("scope section_"), 10)
        self.assertEqual(source.count("path \"operations/"), 100)
        self.assertEqual(source.count("limit?: u32"), 100)
        self.assertEqual(source.count("paginate move |call, page|"), 10)
        self.assertIn("use concord::{json, Json};", source)
        self.assertIn("problem Problem;", source)
        for index in range(10):
            for line in weight.endpoint(index, mix[index]):
                self.assertIn(line, source)

    def test_shape_sample_contains_one_of_each(self):
        source = weight.description(len(weight.SHAPES), weight.SHAPES)
        self.assertEqual(source.count("body json(payload);"), 1)
        self.assertEqual(source.count("status { 404 => None }"), 1)
        self.assertEqual(source.count("paginate move |call, page|"), 1)
        self.assertEqual(source.count("query { q; limit"), 4)


class TimingTests(unittest.TestCase):
    def test_time_report_parses_elapsed_and_max_rss(self):
        template = "\tElapsed (wall clock) time (h:mm:ss or m:ss): {}\n"
        for clock, seconds in (("0:02.25", 2.25), ("3:02.25", 182.25), ("1:03:02", 3782.0)):
            with self.subTest(clock=clock):
                result = weight.parse_time(
                    template.format(clock) + "\tMaximum resident set size (kbytes): 789\n"
                )
                self.assertEqual(result, {"seconds": seconds, "rss_kib": 789})
        self.assertEqual(weight.parse_time(""), {"seconds": None, "rss_kib": None})

    def test_memory_stop_keeps_both_attempts_and_measures_successful_expansion(self):
        weight.OUTPUT.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=weight.OUTPUT) as scratch:
            root = Path(scratch)
            with patch.object(weight.subprocess, "run", return_value=subprocess.CompletedProcess(
                    [], 0, stdout="rustc version", stderr="")):
                run = weight.Run(root / "run", Path("limiter"), "stable", "nightly")
            run.events = None
            attempts = iter((137, 0))

            def fake_build(command, *, cwd, env, stdout, stderr):
                self.assertEqual(cwd, root)
                self.assertEqual(command[:4], ["limiter", "/usr/bin/time", "-v", "-o"])
                self.assertEqual(command[5:], ["rustup", "run", "nightly", "cargo", "rustc",
                                               "--offline", "-j", "4", "--", "-Zunpretty=expanded"])
                self.assertEqual(env["LIMIT_MEM"], "14G")
                self.assertEqual(env["CARGO_TARGET_DIR"], str(root / "cargo"))
                Path(command[4]).write_text("Maximum resident set size (kbytes): 789\n")
                stdout.write("fn sample() {}\n")
                return subprocess.CompletedProcess(command, next(attempts))

            with (patch.object(weight.subprocess, "run", side_effect=fake_build),
                  patch.object(weight.time, "sleep") as pause,
                  patch("builtins.print")):
                success = run.cargo(root, root / "cargo", "sample/expand",
                                    ["rustc", "--", "-Zunpretty=expanded"],
                                    nightly=True, expanded=True, warm=True)
                pause.assert_called_once_with(60)
            self.assertTrue(success)
            first, second = map(json.loads, (run.path / "results.jsonl").read_text().splitlines())
            self.assertEqual((first["exit"], first["memory_stop"]), (137, True))
            self.assertNotIn("lines", first)
            self.assertEqual((second["attempt"], second["lines"], second["rss_kib"]), (2, 1, 789))

    def test_doc_keeps_each_search_index_with_its_own_result(self):
        weight.OUTPUT.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=weight.OUTPUT) as scratch:
            root = Path(scratch)
            target = root / "cargo"
            docs = target / "doc"
            with (patch.object(weight.subprocess, "run", return_value=subprocess.CompletedProcess(
                    [], 0, stdout="rustc version", stderr="")), patch("builtins.print")):
                run = weight.Run(root / "run", Path("limiter"), "stable", "nightly")
                for case in ("first", "second"):
                    docs.mkdir(parents=True)
                    (docs / "index.html").write_text(case)
                    self.assertTrue(run.cargo(root, target, f"{case}/doc", ["doc", "--no-deps"]))
                    self.assertFalse(docs.exists())
                    archived = run.path / "logs" / case / "doc/1-html/index.html"
                    self.assertEqual(archived.read_text(), case)


class ClientTests(unittest.TestCase):
    def test_client_copy_retargets_nested_manifests_and_seeds_empty_libraries(self):
        # Keep test artifacts with the other compile-weight scratch crates.
        weight.OUTPUT.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=weight.OUTPUT) as scratch:
            scratch = Path(scratch)
            original = scratch / "original"
            client = scratch / "copy"
            checkout = scratch / "checkout with spaces"
            (original / "model/src").mkdir(parents=True)
            (original / ".cargo").mkdir()
            (original / ".cargo/config.toml").write_text('[build]\ntarget-dir = "elsewhere"\n')
            (original / "target").mkdir()
            spec = (
                '[package]\nname = "client"\n'
                '[dependencies]\n'
                'concord = { package = "concord-api", path = "/previous/concord-api", features = ["limit"] }\n'
                'model = { path = "model" }\n'
            )
            nested = '[package]\nname = "model"\n[dependencies.concord-testkit]\npath = "../previous/concord-testkit"\n'
            (original / "Cargo.toml").write_text(spec + '\n[lib]\npath = "other.rs"\n')
            (original / "model/Cargo.toml").write_text(nested)
            (original / "model/src/lib.rs").write_text("pub struct Model;")
            (original / "other.rs").write_text("pub struct Client;")

            weight.copy_client(original, client, checkout)
            self.assertFalse((client / ".cargo").exists())
            self.assertFalse((client / "target").exists())
            self.assertEqual((original / "model/Cargo.toml").read_text(), nested)
            changed = (client / "Cargo.toml").read_text()
            self.assertIn(f'path = "{checkout}/concord-api", features = ["limit"]', changed)
            self.assertIn('model = { path = "model" }', changed)
            self.assertIn(f'path = "{checkout}/concord-testkit"', (client / "model/Cargo.toml").read_text())

            seed, packages = weight.make_warm_client(client, scratch / "seed")
            self.assertEqual(packages, ["client", "model"])
            self.assertEqual((seed / "other.rs").read_text(), "")
            self.assertEqual((seed / "model/src/lib.rs").read_text(), "")
            self.assertEqual((seed / "Cargo.toml").read_text(), changed)
            self.assertEqual((client / "other.rs").read_text(), "pub struct Client;")

    def test_real_measurement_evicts_seed_packages_before_timing(self):
        weight.OUTPUT.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=weight.OUTPUT) as scratch:
            root = Path(scratch)
            with patch.object(weight.subprocess, "run", return_value=subprocess.CompletedProcess(
                    [], 0, stdout="rustc version", stderr="")):
                run = weight.Run(root / "run", Path("limiter"), "stable", "nightly")
            client = root / "client"
            seed = root / "seed"
            with (patch.object(weight, "copy_client", return_value=client),
                  patch.object(weight, "make_warm_client", return_value=(seed, ["client", "models"])),
                  patch.object(run, "cargo", return_value=True) as cargo):
                run.real([root / "example"], root / "checkout")
            target = run.path / "clients/example/cargo"
            self.assertEqual([entry.args for entry in cargo.call_args_list], [
                (seed, target, "clients/example/warm/check", ["check", "--lib"]),
                (seed, target, "clients/example/warm/build", ["build", "--lib"]),
                (client, target, "clients/example/reset", ["clean", "--package", "client", "--package", "models"]),
                (client, target, "clients/example/check", ["check", "--lib"]),
                (client, target, "clients/example/build", ["build", "--lib"]),
            ])
