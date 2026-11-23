# macOS app archive

Run `sh packaging/macos/package.sh` on an Apple silicon Mac to build and verify
an ad-hoc-signed app ZIP without publishing. Supply `KVER_UPDATE_PUBLIC_KEY`
to include Sparkle in the candidate.

See the [release guide](../../docs/releasing.md) for signing, smoke checks,
and publication.
