#!/bin/sh
# Reproduce the GPUI Kit skills from the same release as Cargo.toml.
set -eu

tag=v0.11.0
revision=e97fe438129c5e4ccab504f2e6fafd9a4e396bba
source_dir=target/gpui-kit-skills-$tag

if [ ! -d "$source_dir/.git" ]; then
    git clone --depth 1 --filter=blob:none --sparse --branch "$tag" \
        https://github.com/longbridge/gpui-kit.git "$source_dir"
fi
test "$(git -C "$source_dir" rev-parse HEAD)" = "$revision"
git -C "$source_dir" sparse-checkout set \
    skills/gpui-kit skills/gpui-kit-design-guides

for skill in gpui-kit gpui-kit-design-guides; do
    mkdir -p ".agents/skills/$skill"
    cp -R "$source_dir/skills/$skill/." ".agents/skills/$skill/"
done
