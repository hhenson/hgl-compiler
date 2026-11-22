"""Regression cases for release bundle verification and reuse."""

import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "release_bundle", Path(__file__).with_name("release-bundle.py"),
)
release_bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_bundle)


class VerifiedRunTests(unittest.TestCase):
    def setUp(self):
        self.run = {
            "id": 2, "head_sha": "same", "head_branch": "main", "event": "push",
            "status": "in_progress", "run_attempt": 3,
        }
        self.jobs = [
            {"name": name, "conclusion": "success"}
            for name in ("verify", "verify-debug", "hosted-check", "verify-linux", "verify-debug-linux")
        ]
        self.artifacts = [
            {"id": 10, "name": "hgl-macos-arm64-3", "expired": False},
            {"id": 11, "name": "hgl-linux-x86_64-3", "expired": False},
        ]
        runs_query = "branch=main&event=push&head_sha=same&per_page=100"
        self.responses = {
            (f"workflows/release.yml/runs?{runs_query}", "workflow_runs"): [self.run],
            ("runs/2/attempts/3/jobs?per_page=100", "jobs"): self.jobs,
            ("runs/2/artifacts?per_page=100", "artifacts"): self.artifacts,
        }
        self.enterContext(patch.object(release_bundle, "github_items", side_effect=self.github_items))

    def github_items(self, endpoint, key):
        prefix = "repos/test/project/actions/"
        self.assertTrue(endpoint.startswith(prefix))
        return iter(self.responses[endpoint.removeprefix(prefix), key])

    def verified_run(self):
        return release_bundle.verified_run("test/project", "same", "1")

    def test_workspace_test_does_not_gate_verification_reuse(self):
        workspace = {"name": "cpp-macos", "conclusion": None}
        self.jobs.append(workspace)
        for status, outcome in (("in_progress", None), ("completed", "failure"), ("completed", "success")):
            with self.subTest(outcome=outcome):
                workspace["conclusion"] = outcome
                self.run["status"] = status
                self.assertEqual(self.verified_run(), (2, 10, 11))

    def test_every_verifier_has_to_succeed(self):
        for job in self.jobs:
            for outcome in (None, "failure", "skipped"):
                with self.subTest(job=job["name"], outcome=outcome):
                    job["conclusion"] = outcome
                    self.assertIsNone(self.verified_run())
            job["conclusion"] = "success"
        self.jobs.pop()
        self.assertIsNone(self.verified_run())

    def test_verifiers_match_publish_dependencies(self):
        workflow = Path(__file__).resolve().parent.parent / ".github/workflows/release.yml"
        publish = workflow.read_text().partition("\n  publish:\n")[2]
        needs = re.search(r"^    needs: \[([^\]\n]+)\]$", publish, re.MULTILINE)
        self.assertIsNotNone(needs, "expected a flow-style list under jobs.publish.needs")
        dependencies = {job.strip() for job in needs[1].split(",")}

        self.assertEqual(
            dependencies - {"reuse-build", "bundle", "bundle-linux"},
            release_bundle.RELEASE_VERIFIERS,
        )

    def test_artifact_must_be_live_from_the_verified_attempt(self):
        artifact = self.artifacts[1]
        artifact["expired"] = True
        self.assertIsNone(self.verified_run())
        artifact["expired"] = False
        artifact["name"] = "hgl-linux-x86_64-2"
        self.assertIsNone(self.verified_run())

    def test_run_must_match_source_and_exclude_current_run(self):
        for field, wrong in (("head_sha", "different"), ("head_branch", "branch"),
                             ("event", "workflow_dispatch"), ("id", 1), ("status", "queued")):
            with self.subTest(field=field):
                previous = self.run[field]
                self.run[field] = wrong
                self.assertIsNone(self.verified_run())
                self.run[field] = previous


class VerifyPathsTests(unittest.TestCase):
    def setUp(self):
        root = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        self.build = root / "forbidden-build"
        self.build.mkdir()
        self.bundle = root / "bundle"
        self.bundle.mkdir()

    def test_forbidden_path_in_an_include_flag(self):
        (self.bundle / "arguments.txt").write_text(f"-I{self.build}/x\n")

        with self.assertRaisesRegex(ValueError, r"build path in arguments\.txt:"):
            release_bundle.verify_paths(self.bundle, [self.build])

    def test_home_in_a_license_url_is_not_a_build_path(self):
        (self.bundle / "LICENSE.txt").write_text("https://example.com/crates/home/license\n")

        release_bundle.verify_paths(self.bundle, [self.build])

    @unittest.skipUnless(
        sys.platform == "linux" and shutil.which("cc") and shutil.which("objcopy"),
        "needs a Linux C compiler and GNU objcopy",
    )
    def test_forbidden_path_in_gnu_compressed_debug_info(self):
        source = self.build / "sample.c"
        source.write_text("int twice(int value) { return value + value; }\n")
        binary = self.bundle / "library.so"
        subprocess.run(
            ["cc", "-g", "-gdwarf-4", "-fPIC", "-shared", source.name, "-o", binary],
            cwd=self.build, check=True,
        )
        subprocess.run(["objcopy", "--compress-debug-sections=zlib-gnu", binary], check=True)
        # objcopy leaves small sections uncompressed. Exercise a file with
        # only legacy names so a plain .debug_ section cannot trigger decoding.
        subprocess.run(["objcopy", "--remove-section=.debug_*", binary], check=True)
        data = binary.read_bytes()
        self.assertIn(b".zdebug_info", data)
        self.assertNotIn(b".debug_", data)
        self.assertNotIn(os.fsencode(self.build), data)

        with self.assertRaisesRegex(ValueError, r"build path in library\.so:"):
            release_bundle.verify_paths(self.bundle, [self.build])


if __name__ == "__main__":
    unittest.main()
