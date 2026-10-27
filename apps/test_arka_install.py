import importlib.machinery
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
loader = importlib.machinery.SourceFileLoader("arka_install", os.path.join(HERE, "arka-install"))
spec = importlib.util.spec_from_loader("arka_install", loader)
arka = importlib.util.module_from_spec(spec)
loader.exec_module(arka)

SAMPLE = """[Context]
shared=network;ipc;
sockets=x11;pulseaudio;wayland;fallback-x11;
devices=dri;all;
filesystems=xdg-download:ro;xdg-documents;home;~/Projects:ro;!xdg-music;

[Session Bus Policy]
org.freedesktop.Notifications=talk
org.kde.StatusNotifierWatcher=talk
org.example.Hidden=none
"""


class Permissions(unittest.TestCase):
    def test_wide_and_narrow_are_split(self):
        wide, narrow = arka.parse_permissions(SAMPLE)
        self.assertIn("Read and change your whole home folder", wide)
        self.assertTrue(any("every device" in s for s in wide))
        self.assertTrue(any("X11" in s for s in wide))
        self.assertIn("Connect to the internet", narrow)
        self.assertIn("Read your Downloads folder", narrow)
        self.assertIn("Read and change your Documents folder", narrow)
        self.assertIn("Read the folder ~/Projects", narrow)
        self.assertIn("Talk to 2 other services on the desktop bus", narrow)

    def test_negated_permissions_are_not_listed(self):
        _, narrow = arka.parse_permissions(SAMPLE)
        self.assertFalse(any("Music" in s for s in narrow))

    def test_no_permissions(self):
        self.assertEqual(arka.parse_permissions(""), ([], []))
        self.assertIn("asks for no extra access", arka.describe("X", "", ""))

    def test_description_says_what_the_list_is_not(self):
        text = arka.describe("VLC", "Plays video.", SAMPLE)
        self.assertIn("Broad access:", text)
        self.assertIn("don't make it safe", text)


class Catalog(unittest.TestCase):
    def test_catalog_file_is_valid_and_unique(self):
        import json
        with open(os.path.join(HERE, "apps.json")) as f:
            apps = json.load(f)
        ids = [a["id"] for a in apps]
        self.assertEqual(len(ids), len(set(ids)))
        for a in apps:
            self.assertTrue({"id", "name", "about", "category"} <= a.keys())
            self.assertIn(".", a["id"])

    def test_every_capsule_app_is_on_the_list(self):
        # Capsule installs through arka, so an app on its grid must be on arka's list too.
        import json, re
        capsule = os.path.join(HERE, "..", "arka-shell", "arka-capsule", "src", "main.rs")
        with open(capsule) as f:
            ids = re.findall(r'CatalogApp \{[^}]*?id: "([^"]+)"', f.read())
        with open(os.path.join(HERE, "apps.json")) as f:
            listed = {a["id"] for a in json.load(f)}
        self.assertGreaterEqual(len(ids), 10)
        self.assertEqual([i for i in ids if i not in listed], [])

    def test_find_by_name_or_id_ignores_case(self):
        cat = [{"id": "org.videolan.VLC", "name": "VLC", "about": "", "category": "Media"}]
        self.assertEqual(arka.find("vlc", cat)["id"], "org.videolan.VLC")
        self.assertEqual(arka.find("ORG.VIDEOLAN.VLC", cat)["name"], "VLC")
        self.assertIsNone(arka.find("nope", cat))


if __name__ == "__main__":
    unittest.main()
