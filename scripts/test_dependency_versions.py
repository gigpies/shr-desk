import tempfile
from pathlib import Path
import unittest

from check_dependency_versions import dependencies, report


class DependencyInventoryTests(unittest.TestCase):
    def test_registry_filter_alias_targets_and_vendor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text("""[dependencies]
internal = { version = "=1.0.0", registry = "frozen" }
renamed = { package = "real", version = "1" }
local = { path = "local" }
remote = { git = "https://example.test/repo" }
[target.'cfg(unix)'.dependencies]
unix = "1"
""")
            vendor = root / "vendor" / "panel"
            vendor.mkdir(parents=True)
            (vendor / "Cargo.toml").write_text('[dependencies]\nshared = "1"\n')
            registry = "registry+https://github.com/rust-lang/crates.io-index"
            (root / "Cargo.lock").write_text("\n".join(
                f'[[package]]\nname = "{name}"\nversion = "1.0.0"\nsource = "{registry}"'
                for name in ("real", "unix", "shared")))
            self.assertEqual(dependencies(root), {"real": ["1.0.0"], "shared": ["1.0.0"], "unix": ["1.0.0"]})

    def test_reports_multiple_locked_versions_and_unlocked_optional_dependency(self):
        text = report({"a": ["1.0.0", "2.0.0"], "b": [], "c": ["2.0.0"]}, lambda _: "2.0.0")
        self.assertIn("| a | 1.0.0, 2.0.0 | 2.0.0 | review available versions |", text)
        self.assertIn("| b | not locked |", text)
        self.assertIn("| c | 2.0.0 | 2.0.0 | current |", text)

    def test_registry_failure_is_not_reported_as_success(self):
        def unavailable(_):
            raise OSError("registry unavailable")
        with self.assertRaises(OSError):
            report({"a": ["1.0.0"]}, unavailable)


if __name__ == "__main__":
    unittest.main()
