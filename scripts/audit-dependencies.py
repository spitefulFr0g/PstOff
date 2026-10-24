#!/usr/bin/env python3
"""Check Cargo metadata from sandboxed cargo; fail closed on unaudited licenses."""
import json
import sys

# Exact expressions inspected for the foundation lockfile. New expressions need
# review, even if a substring happens to say MIT.
APPROVED = {
    "MIT", "MIT OR Apache-2.0", "Apache-2.0 OR MIT", "Unlicense OR MIT",
    "0BSD OR MIT OR Apache-2.0", "MIT OR Zlib OR Apache-2.0",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
}
metadata = json.load(sys.stdin)
errors = []
for package in metadata["packages"]:
    if package.get("license") not in APPROVED:
        errors.append(f"Unreviewed license: {package['name']} {package.get('license')}")
    if package["name"] == "gpui":
        errors.append("GPUI must remain outside Linux workspace resolution")
    if package["name"] == "outlook-pst" and package.get("source") != (
        "git+https://github.com/microsoft/outlook-pst-rs?"
        "rev=cfb721daee538acc50d2bad6faac5ce724f6037d#cfb721daee538acc50d2bad6faac5ce724f6037d"
    ):
        errors.append("Microsoft independent reader pin changed without audit")
if errors:
    sys.exit("\n".join(errors))
print(f"Dependency audit passed: {len(metadata['packages'])} packages; GPUI absent")
