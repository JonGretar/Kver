"""Validate public updater configuration and add it to a completed app plist."""

import argparse
import base64
import os
import pathlib
import plistlib
import urllib.parse

DEFAULT_FEED = "https://github.com/JonGretar/Kver/releases/latest/download/appcast.xml"


def update_settings(public_key, feed_url):
    try:
        key = base64.b64decode(public_key, validate=True)
    except ValueError as error:
        raise ValueError("KVER_UPDATE_PUBLIC_KEY must be a base64 Ed25519 public key.") from error
    if len(key) != 32:
        raise ValueError("KVER_UPDATE_PUBLIC_KEY must decode to 32 bytes.")
    url = urllib.parse.urlsplit(feed_url)
    if (
        url.scheme != "https"
        or not url.hostname
        or url.username is not None
        or url.password is not None
        or url.fragment
        or any(character.isspace() for character in feed_url)
    ):
        raise ValueError("KVER_UPDATE_FEED_URL must be an HTTPS URL without credentials or a fragment.")
    return {
        "SUFeedURL": feed_url,
        "SUPublicEDKey": public_key,
        # Verify before extracting code, and authenticate the feed as well.
        "SUVerifyUpdateBeforeExtraction": True,
        "SURequireSignedFeed": True,
        # Offer optional background checks, but require approval to install.
        "SUAllowsAutomaticUpdates": False,
        "LSMinimumSystemVersion": "12.0",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plist", nargs="?", type=pathlib.Path)
    args = parser.parse_args()
    try:
        settings = update_settings(
            os.environ.get("KVER_UPDATE_PUBLIC_KEY", ""),
            os.environ.get("KVER_UPDATE_FEED_URL", DEFAULT_FEED),
        )
        if args.plist:
            with args.plist.open("rb") as source:
                info = plistlib.load(source)
            info.update(settings)
            with args.plist.open("wb") as destination:
                plistlib.dump(info, destination)
    except (ValueError, OSError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
