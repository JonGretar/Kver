#!/bin/sh
# Reproduce the GPUI Kit skills from the same release as Cargo.toml.
set -eu

revision=99576db58ec8103417c41a092e891b4022d24e7e # v0.8.1
source_dir=target/gpui-kit-skills-upstream

if [ ! -d "$source_dir/.git" ]; then
    git clone --depth 1 --filter=blob:none --sparse --branch v0.8.1 \
        https://github.com/longbridge/gpui-kit.git "$source_dir"
fi
test "$(git -C "$source_dir" rev-parse HEAD)" = "$revision"
git -C "$source_dir" sparse-checkout set \
    skills/gpui-kit skills/gpui-kit-design-guides

for skill in gpui-kit gpui-kit-design-guides; do
    mkdir -p ".agents/skills/$skill"
    cp -R "$source_dir/skills/$skill/." ".agents/skills/$skill/"
done
