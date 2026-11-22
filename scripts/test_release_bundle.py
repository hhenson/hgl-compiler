"""Regression cases for release bundle path verification."""

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "release_bundle", Path(__file__).with_name("release-bundle.py"),
)
release_bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_bundle)


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
