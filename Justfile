default:
    @just --list

# Open the debug reader and choose a PDF in the window.
run:
    corgi run

# Open the optimized reader and choose a PDF in the window.
run-release:
    corgi run --release

# Build and verify the unsigned Apple silicon app archive.
build-macos-release:
    sh packaging/macos/package.sh
