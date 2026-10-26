#!/usr/bin/env python3
"""Check Windows-filtered Cargo metadata from sandboxed cargo; fail closed on unaudited licenses."""
import json
import sys

# Exact expressions inspected for the foundation lockfile. New expressions need
# review, even if a substring happens to say MIT.
APPROVED = {
    "MIT", "MIT OR Apache-2.0", "Apache-2.0 OR MIT", "Unlicense OR MIT",
    "0BSD OR MIT OR Apache-2.0", "MIT OR Zlib OR Apache-2.0",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
    # Reviewed from locked ryu 1.0.20 and version_check 0.9.5 license files.
    "Apache-2.0 OR BSL-1.0", "MIT/Apache-2.0",
    # Windows GPUI graph; exact expressions and notice sources recorded in
    # docs/gui-dependency-audit.md. Distribution retains dependency notices.
    "Apache-2.0", "Apache-2.0 / MIT", "Apache-2.0/MIT",
    "BSD-2-Clause", "BSD-2-Clause OR Apache-2.0 OR MIT", "BSD-3-Clause",
    "BSD-3-Clause OR Apache-2.0", "CC0-1.0 OR Apache-2.0",
    "CC0-1.0 OR MIT-0 OR Apache-2.0", "MIT OR Apache-2.0 OR Zlib",
    "Unicode-3.0", "Zlib", "Zlib OR Apache-2.0 OR MIT",
}
# Cargo emits UTF-8 JSON regardless of the Windows host's active code page.
metadata = json.load(sys.stdin.buffer)
errors = []
for package in metadata["packages"]:
    if package.get("license") not in APPROVED:
        errors.append(f"Unreviewed license: {package['name']} {package.get('license')}")
    if package["name"] == "outlook-pst" and package.get("source") != (
        "git+https://github.com/microsoft/outlook-pst-rs?"
        "rev=cfb721daee538acc50d2bad6faac5ce724f6037d#cfb721daee538acc50d2bad6faac5ce724f6037d"
    ):
        errors.append("Microsoft independent reader pin changed without audit")
    if package["name"].startswith("gpui-pre") and package["version"] != "0.3.9":
        errors.append(f"GPUI snapshot pin changed without audit: {package['name']} {package['version']}")
if errors:
    sys.exit("\n".join(errors))
print(f"Dependency audit passed: {len(metadata['packages'])} Windows packages")
