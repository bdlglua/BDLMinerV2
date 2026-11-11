"""Offline regression tests for release packaging and usage reporting."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
import subprocess
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import make_release
import miner_stats
import build_output


class ReleaseToolsTests(unittest.TestCase):
    def test_slot_report_warns_near_a_full_update_slot(self):
        def entry(kind, subtype, offset, size):
            return b"\xaa\x50" + bytes([kind, subtype]) + offset.to_bytes(4, "little") + size.to_bytes(4, "little") + b"\0" * 20
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            table = root / "partitions.bin"
            # nvs (data), ota_0 and ota_1 of 1920 KB, a factory app that does not count.
            table.write_bytes(entry(1, 2, 0x9000, 0x5000) + entry(0, 0x10, 0x10000, 1920 * 1024)
                              + entry(0, 0x11, 0x1F0000, 1920 * 1024) + entry(0, 0, 0x3D0000, 64 * 1024) + b"\xff" * 32)
            self.assertEqual(make_release.app_slot(table), 1920 * 1024)
            firmware = root / "firmware.bin"
            for size, warned in ((1788 * 1024, False), (1840 * 1024, True)):
                firmware.write_bytes(b"\0" * size)
                out = io.StringIO()
                with contextlib.redirect_stdout(out):
                    make_release.slot_report("board", firmware, table)
                self.assertIn("of its 1920 KB update slot", out.getvalue())
                self.assertEqual("::warning" in out.getvalue(), warned)

    def test_packaging_failure_keeps_the_actual_tool_diagnostic(self):
        output = io.StringIO()
        error = subprocess.CalledProcessError(2, ["esptool"], output=b"tool output", stderr=b"missing input image")
        with (patch.object(make_release.subprocess, "run", side_effect=error),
              contextlib.redirect_stderr(output)):
            with self.assertRaises(subprocess.CalledProcessError):
                make_release.checked(["esptool"])
        self.assertIn("missing input image", output.getvalue())

    def test_layout_generation_precedes_build_from_a_clean_checkout(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            firmware = root / "firmware.bin"
            bootloader = root / "bootloader.bin"
            layout = ("esp32", "dio", "40m", "4MB", [("0x1000", str(bootloader)), ("0x10000", str(firmware))])
            def discover(env):
                # Reproduce a first envdump invalidating generated build files.
                firmware.unlink(missing_ok=True)
                bootloader.unlink(missing_ok=True)
                return layout
            def compile_(*args):
                firmware.write_bytes(b"firmware")
                bootloader.write_bytes(b"bootloader")
            def merge(command, **kwargs):
                self.assertTrue(firmware.is_file() and bootloader.is_file())
                Path(command[command.index("-o") + 1]).write_bytes(b"factory")
            with (patch.object(make_release, "ROOT", str(root)),
                  patch.object(make_release, "envs", return_value=["alpha"]),
                  patch.object(make_release, "version", return_value="1.1.2"),
                  patch.object(make_release, "signing_key", return_value=None),
                  patch.object(make_release, "run", side_effect=compile_),
                  patch.object(make_release, "flash_layout", side_effect=discover),
                  patch.object(make_release.subprocess, "run", side_effect=merge),
                  patch.object(sys, "argv", ["make_release.py", "alpha"]),
                  contextlib.redirect_stdout(io.StringIO())):
                make_release.main()
            self.assertTrue((root / "release/1.1.2/alpha-factory.bin").is_file())

    def test_partial_unsigned_rebuild_removes_stale_signature_and_keeps_checksums(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            out = root / "release" / "1.0.8"
            out.mkdir(parents=True)
            firmware = root / "firmware.bin"
            firmware.write_bytes(b"new firmware")
            (out / "alpha-firmware.bin").write_bytes(b"old firmware")
            (out / "alpha-firmware.bin.sig").write_bytes(b"old signature")
            (out / "beta-firmware.bin").write_bytes(b"other firmware")
            (out / "beta-factory.bin").write_bytes(b"other factory")
            (out / "boards.json").write_text(json.dumps({"alpha": "esp32", "beta": "esp32c3"}))

            def merge(command, **kwargs):
                Path(command[command.index("-o") + 1]).write_bytes(b"new factory")

            layout = ("esp32", "dio", "40m", "4MB", [("0x10000", str(firmware))])
            with (patch.object(make_release, "ROOT", str(root)),
                  patch.object(make_release, "envs", return_value=["alpha", "beta"]),
                  patch.object(make_release, "version", return_value="1.0.8"),
                  patch.object(make_release, "signing_key", return_value=None),
                  patch.object(make_release, "run"),
                  patch.object(make_release, "flash_layout", return_value=layout),
                  patch.object(make_release.subprocess, "run", side_effect=merge),
                  patch.object(sys, "argv", ["make_release.py", "alpha"]),
                  contextlib.redirect_stdout(io.StringIO())):
                make_release.main()

            with self.subTest("stale signature"):
                self.assertFalse((out / "alpha-firmware.bin.sig").exists())
            self.assertEqual((out / "alpha-firmware.bin").read_bytes(), b"new firmware")
            checksums = dict(line.split("  ")[::-1] for line in (out / "SHA256SUMS.txt").read_text().splitlines())
            binaries = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob("*.bin")}
            with self.subTest("checksums for all boards"):
                self.assertEqual(checksums, binaries)
            self.assertEqual(json.loads((out / "boards.json").read_text()), {"alpha": "esp32", "beta": "esp32c3"})
            self.assertEqual(json.loads((out / "update.json").read_text())["assets"], [])

    def test_update_manifest_advertises_only_signed_firmware_with_matching_version(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            for name in ("alpha-firmware.bin", "alpha-firmware.bin.sig", "alpha-factory.bin",
                         "beta-firmware.bin", "orphan-firmware.bin.sig"):
                (out / name).write_bytes(b"test")
            with patch.dict(make_release.os.environ, {"GITHUB_REPOSITORY": "bdlglua/BDLMinerV2"}):
                make_release.write_update_manifest(temp, "1.0.11")
            manifest = json.loads((out / "update.json").read_text())
            self.assertEqual(manifest["tag_name"], "v1.0.11")
            self.assertEqual([asset["name"] for asset in manifest["assets"]],
                             ["alpha-firmware.bin", "alpha-firmware.bin.sig"])
            for asset in manifest["assets"]:
                self.assertEqual(asset["browser_download_url"],
                                 "https://github.com/bdlglua/BDLMinerV2/releases/download/v1.0.11/" + asset["name"])

    def test_usage_report_accepts_two_and_three_component_versions(self):
        releases = [
            {"tag_name": "v1.0", "assets": [{"name": "ping-esp32.txt", "download_count": 2}]},
            {"tag_name": "v1.0.1", "assets": [{"name": "ping-esp32.txt", "download_count": 3}]},
            {"tag_name": "v1.0.10", "assets": [{"name": "ping-esp32.txt", "download_count": 4}]},
        ]
        with tempfile.TemporaryDirectory() as temp:
            state = Path(temp) / "stats.json"
            output = io.StringIO()
            with (patch.object(miner_stats, "STATE", str(state)),
                  patch.object(miner_stats, "releases", return_value=releases),
                  contextlib.redirect_stdout(output)):
                miner_stats.main()
            rows = output.getvalue().splitlines()[1:4]
            self.assertEqual([row.split()[0] for row in rows], ["1.0.10", "1.0.1", "1.0"])
            self.assertEqual(len(json.loads(state.read_text())["counts"]), 3)

    def test_failed_rebuild_preserves_every_previous_artifact(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            out = root / "release/1.5.2"
            out.mkdir(parents=True)
            original = {"alpha-firmware.bin": b"old app", "alpha-firmware.bin.sig": b"valid signature",
                        "alpha-factory.bin": b"old factory", "update.json": b"old manifest"}
            for name, content in original.items():
                (out / name).write_bytes(content)
            def fail(targets, staged, version, key):
                (Path(staged) / "alpha-firmware.bin").write_bytes(b"new app")
                raise RuntimeError("signing failed on a later board")
            with (patch.object(make_release, "ROOT", str(root)),
                  patch.object(make_release, "envs", return_value=["alpha"]),
                  patch.object(make_release, "version", return_value="1.5.2"),
                  patch.object(make_release, "signing_key", return_value=None),
                  patch.object(make_release, "build_release", side_effect=fail),
                  patch.object(sys, "argv", ["make_release.py"])):
                with self.assertRaisesRegex(RuntimeError, "signing failed"):
                    make_release.main()
            self.assertEqual({p.name: p.read_bytes() for p in out.iterdir()}, original)
            self.assertEqual(sorted(p.name for p in out.parent.iterdir()), ["1.5.2"])

    def test_invalid_version_and_board_never_change_configuration(self):
        for args in (["--version"], ["--version", "../bad"], ["--version", "1.5.3", "typo-board"]):
            with (patch.object(sys, "argv", ["make_release.py", *args]),
                  patch.object(make_release, "set_version", wraps=make_release.set_version) as setter,
                  contextlib.redirect_stderr(io.StringIO())):
                with self.assertRaises(SystemExit):
                    make_release.main()
                if "typo-board" in args or args == ["--version"]:
                    setter.assert_not_called()

    def test_wrong_key_never_replaces_the_last_verified_signature(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "src").mkdir()
            key, other = root / "key.pem", root / "other.pem"
            for path in (key, other):
                subprocess.run(["openssl", "genpkey", "-algorithm", "EC", "-pkeyopt",
                                "ec_paramgen_curve:P-256", "-out", str(path)], check=True, capture_output=True)
            public = subprocess.run(["openssl", "pkey", "-in", str(key), "-pubout"],
                                    check=True, capture_output=True, text=True).stdout
            (root / "src/update_key.h").write_text("\n".join(json.dumps(line + "\n") for line in public.splitlines()))
            app = Path(temp) / "app.bin"
            app.write_bytes(b"firmware")
            signature = app.with_suffix(".bin.sig")
            with patch.object(make_release, "ROOT", str(root)):
                make_release.sign(str(key), str(app))
                verified = signature.read_bytes()
                self.assertGreater(len(verified), 0)
                with self.assertRaises(subprocess.CalledProcessError):
                    make_release.sign(str(other), str(app))
            self.assertEqual(signature.read_bytes(), verified)
            self.assertFalse(list(root.glob(".sign-*")))

    def test_missing_explicit_signing_key_does_not_silently_build_unsigned(self):
        with patch.dict(make_release.os.environ, {"RELEASE_SIGNING_KEY_FILE": "/missing/test-key.pem"}):
            with self.assertRaises(FileNotFoundError):
                make_release.signing_key()


    def test_output_publish_failure_rolls_back_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "site"
            out.mkdir()
            (out / "index.html").write_text("previous")
            replace = build_output.os.replace
            def fail_publish(source, target):
                if Path(source).name == "output":
                    raise OSError("disk error")
                return replace(source, target)
            with patch.object(build_output.os, "replace", side_effect=fail_publish):
                with self.assertRaisesRegex(OSError, "disk error"):
                    with build_output.staged_directory(out) as staged:
                        (staged / "index.html").write_text("new")
            self.assertEqual((out / "index.html").read_text(), "previous")

    def test_usage_report_does_not_invent_activity_for_new_or_reset_assets(self):
        releases = [{"tag_name": "v1.5.2", "assets": [
            {"name": "ping-known.txt", "download_count": 105},
            {"name": "ping-reset.txt", "download_count": 2},
            {"name": "ping-new.txt", "download_count": 1000},
        ]}]
        with tempfile.TemporaryDirectory() as temp:
            state = Path(temp) / "stats.json"
            state.write_text(json.dumps({"time": 100, "counts": {"1.5.2 known": 100, "1.5.2 reset": 50}}))
            output = io.StringIO()
            with (patch.object(miner_stats, "STATE", str(state)),
                  patch.object(miner_stats, "releases", return_value=releases),
                  patch.object(miner_stats.time, "time", return_value=86500),
                  contextlib.redirect_stdout(output)):
                miner_stats.main()
            rows = {row.split()[1]: row.split()[2:] for row in output.getvalue().splitlines()[1:4]}
            self.assertEqual(rows["known"], ["105", "5.0"])
            self.assertEqual(rows["reset"], ["2"])
            self.assertEqual(rows["new"], ["1000"])
            self.assertIn("about 5 active miners", output.getvalue())

    def test_corrupt_usage_snapshot_recovers_and_failed_save_keeps_old_baseline(self):
        releases = [{"tag_name": "v1.5.2", "assets": [{"name": "ping-esp32.txt", "download_count": 10}]}]
        with tempfile.TemporaryDirectory() as temp:
            state = Path(temp) / "stats.json"
            for damaged in ('{truncated', '[]', '{"time":NaN,"counts":{}}', '{"time":1,"counts":{"bad":"10"}}'):
                state.write_text(damaged)
                with (patch.object(miner_stats, "STATE", str(state)),
                      patch.object(miner_stats, "releases", return_value=releases),
                      contextlib.redirect_stdout(io.StringIO()),
                      contextlib.redirect_stderr(io.StringIO()) as errors):
                    miner_stats.main()
                self.assertIn("invalid", errors.getvalue())
                self.assertEqual(json.loads(state.read_text())["counts"], {"1.5.2 esp32": 10})
            original = state.read_bytes()
            with (patch.object(miner_stats, "STATE", str(state)),
                  patch.object(miner_stats, "releases", return_value=releases),
                  patch.object(miner_stats.os, "replace", side_effect=OSError("disk error")),
                  contextlib.redirect_stdout(io.StringIO())):
                with self.assertRaisesRegex(OSError, "disk error"):
                    miner_stats.main()
            self.assertEqual(state.read_bytes(), original)
            self.assertEqual(list(state.parent.iterdir()), [state])


if __name__ == "__main__":
    unittest.main()
