# Packaging checks

From the repository root, run `python3 -m unittest discover -s packaging/tests`.
The Sparkle signature tests require macOS. Archive rejection tests do not build
packages; run the platform packager to verify an actual archive.
