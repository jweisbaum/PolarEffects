"""Reject incomplete or mixed-version releases before publication."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("assets", Path(__file__).with_name("check-release-assets.py"))
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)

NAMES = [
    "PolarEffects_0.1.0_x64.dmg", "PolarEffects_0.1.0_aarch64.dmg",
    "PolarEffects_0.1.0_x64_en-US.msi", "PolarEffects_0.1.0_x64-setup.exe",
    "PolarEffects_0.1.0_arm64-setup.exe", "PolarEffects_0.1.0_amd64.AppImage",
    "polareffects_0.1.0_amd64.deb", "PolarEffects-0.1.0-1.x86_64.rpm",
]


class ReleaseAssets(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        for name in NAMES:
            (self.directory / name).write_bytes(b"abc")

    def test_complete_release_hashes_payloads_in_name_order_and_is_repeatable(self):
        (self.directory / "PolarEffects-documentation.zip").write_bytes(b"documentation")
        assets.main(self.directory, "0.1.0")
        checksum = self.directory / "SHA256SUMS"
        original = checksum.read_bytes()
        lines = checksum.read_text().splitlines()
        names = sorted(NAMES + ["PolarEffects-documentation.zip"])
        self.assertEqual([line.split("  ")[1] for line in lines], names)
        for line in lines:
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((self.directory / name).read_bytes()).hexdigest())
        assets.main(self.directory, "0.1.0")
        self.assertEqual(checksum.read_bytes(), original)

    def test_each_required_installer_is_indispensable(self):
        for name in NAMES:
            with self.subTest(name=name):
                file = self.directory / name
                file.unlink()
                with self.assertRaisesRegex(SystemExit, "Missing release installers"):
                    assets.main(self.directory, "0.1.0")
                file.write_bytes(b"abc")

    def test_rejects_empty_assets(self):
        (self.directory / NAMES[0]).write_bytes(b"")
        with self.assertRaisesRegex(SystemExit, "Empty release asset"):
            assets.main(self.directory, "0.1.0")

    def test_rejects_stale_installers_in_an_otherwise_complete_draft(self):
        stale = NAMES[0].replace("0.1.0", "0.0.9")
        (self.directory / stale).write_bytes(b"abc")
        with self.assertRaisesRegex(SystemExit, "does not match PolarEffects 0.1.0"):
            assets.main(self.directory, "0.1.0")


if __name__ == "__main__":
    unittest.main()
