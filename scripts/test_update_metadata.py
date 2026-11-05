import base64
import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).with_name("update-metadata.py")
spec = importlib.util.spec_from_file_location("update_metadata", SCRIPT)
update_metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update_metadata)

# Public test data, not a production key.
PUBLIC_KEY = base64.b64encode(bytes(range(32))).decode()


class UpdateMetadataTests(unittest.TestCase):
    def test_requires_signature_verification_and_user_approved_installation(self):
        settings = update_metadata.update_settings(PUBLIC_KEY, update_metadata.DEFAULT_FEED)
        self.assertEqual(settings["SUPublicEDKey"], PUBLIC_KEY)
        self.assertEqual(settings["SUFeedURL"], update_metadata.DEFAULT_FEED)
        self.assertTrue(settings["SUVerifyUpdateBeforeExtraction"])
        self.assertTrue(settings["SURequireSignedFeed"])
        self.assertFalse(settings["SUAllowsAutomaticUpdates"])
        # Let Sparkle ask for background-check consent and persist the answer.
        self.assertNotIn("SUEnableAutomaticChecks", settings)

    def test_rejects_missing_or_malformed_keys(self):
        for key in ("", "not base64", "!!!!", base64.b64encode(bytes(64)).decode()):
            with self.subTest(key=key), self.assertRaises(ValueError):
                update_metadata.update_settings(key, update_metadata.DEFAULT_FEED)

    def test_rejects_insecure_or_credential_bearing_feeds(self):
        for url in (
            "http://example.com/appcast.xml",
            "file:///appcast.xml",
            "https:///appcast.xml",
            "https://token@example.com/appcast.xml",
            "https://example.com/appcast.xml#fragment",
            "https://example.com/appcast.xml\n",
        ):
            with self.subTest(url=url), self.assertRaises(ValueError):
                update_metadata.update_settings(PUBLIC_KEY, url)

    def test_supports_an_independent_public_feed(self):
        url = "https://updates.example.com/kver/appcast.xml?channel=stable&arch=arm64"
        self.assertEqual(update_metadata.update_settings(PUBLIC_KEY, url)["SUFeedURL"], url)


if __name__ == "__main__":
    unittest.main()
