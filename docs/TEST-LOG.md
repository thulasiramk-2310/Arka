# Test log

What has been tested, where, and what has not. The rule is in [TESTING.md](TESTING.md): nothing is
"done" until it has been seen working in a booted VM.

**How to read this.** The first table copies each feature's own test statement from the README,
word for word, ordered by the newest date it mentions. Nothing is added to it that the README does
not already say. The second section records what was checked on 2026-11-01, and says where.
"Not yet on hardware" means exactly that: no real machine has run it.

## From the README (copied, not reworded)

| Feature | What the README says about testing |
|---|---|
| Installer ISO with disk encryption (first version) | Tested in a VM on 2026-10-12: install, unlocking at boot, setup with all six settings configured, the desktop, and automatic rollback after a broken update. Not tested on real hardware. |
| Fewer false alarms | Tested in a VM on 2026-10-14. |
| Signed updates from ghcr.io | Not yet on hardware. |
| Dock | Tested in a VM (2026-10-18). Not yet on hardware. |
| Launcher | Tested in a VM (2026-10-20) with mouse, keyboard and the Orca screen reader, in Light and Dark, at 150% scaling, and on two virtual screens. Not yet on hardware. |
| Top bar on every screen | Tested in a VM with two virtual screens (2026-10-20). Not yet with real monitors. |
| Popups with cut corners | Tested in a VM (2026-10-20). |
| Lock screen in Light | Tested in a VM (2026-10-20). |
| Convert files from the right-click menu | Tested in a VM (2026-10-19 and 2026-10-20). Not yet on hardware. |
| Remove hidden details | Tested in a VM (2026-10-20): a photo's camera and author, a video's GPS position and a PDF's author were gone from the copies. |
| VARA and Git | Tested in a VM (2026-10-20). Not yet on hardware. |
| KDE Connect is off by default | Tested in a VM (2026-10-20 and 2026-10-25): it started and stopped, port 1716 opened and closed, and it came back after logging in again. Not tested with a real phone. |
| Window snapping | Tested in a VM (2026-10-25) with the keyboard and by dragging. Not yet on a touchpad. |
| Move the dock | Tested in a VM (2026-10-25): each choice moved the dock and one dock kept running. |
| Night light | Tested in a VM (2026-10-25): KWin reported Night light at 4500 K when on and 6500 K when off. |
| Reduce motion | Tested in a VM (2026-10-25): the setting is written and removed, KDE's change notice is sent on the session bus, and Settings shows the real value when reopened. |
| Edit from the right-click menu | Tested in a VM (2026-10-26) with a real mouse and keyboard: the menu entries for rotate right and merge, and the crop (opened from the edit wheel), add a background and trim (MP3) windows. Black out's drag was checked in the VM, but its button was only clicked outside it. The other sound and video formats were not tested. Not yet on hardware. |
| Format wheel and edit wheel | Tested in a VM (2026-10-26) with a real drag from Dolphin: the format wheel converted a picture to WebP and left the original alone, and the edit wheel opened the crop window. Drags from Gwenview and Firefox were tested on 2026-10-19. That was the earlier round look; the octagon look (2026-10-27) has only been checked in screenshots. Not yet with a real touchpad. |
| Make files smaller, put them in a ZIP, and turn Word files into PDFs | Tested with real files on the development laptop (2026-11-01), not yet in a VM or on hardware. The smaller-video step needs the OpenH264 encoder that ArkaOS ships and that laptop lacks, so it is untested. |
| New setup and a Light/Dark look (Fedora 44 candidate) | Tested in a VM in both looks. Not yet on hardware. |
| Natural touchpad scrolling by default | Tested in a VM with a virtual touchpad. Not yet on a real one. |
| Installing apps shows what they can reach first | Not tested in a VM or on hardware. |
| Keyboard shortcuts page | Not tested with real key presses. |
| Three spaces | Not tested in a VM or on hardware. |

## Checked on 2026-11-01

All of this was seen in a booted VM running the stick image built from commit `506bc2d` (Fedora 44 base,
kernel 7.2.13, XFS root). Nothing here has run on real hardware.

- **Setup, all 9 pages**, in Dark with Green and Blue, and in Light with Blue: each page rendered, the header
  logo turned blue when Blue was chosen, the time zone step showed a live clock line (Europe/Berlin read one
  hour ahead of UTC), and the account was created. The time zone and the accent were applied after the reboot.
- **Boot splash** showed the new logo (green, as designed). **Login screen** showed the logo and the Berlin time.
- **First desktop**: blue logo, blue wallpaper, no tagline, and one search button, one status pill and one
  spaces ridge (the duplicate-widget bug was not present). The same on the second login after a reboot.
- **Settings > Appearance > Accent colour**: Green and Blue switched the logo, wallpaper line and spaces marker
  within seconds, driven from the keyboard. Green stayed after a reboot.
- **Converting files with the format wheel and Make smaller**, run on real files inside the image's own sandbox:
  photo 6.3 MB to 1.8 MB (camera and GPS data gone), PNG 6.9 to 6.5 MB, WAV 882 KB to 21 KB (MP3), song 289 to
  49 KB, **video 3.7 MB to 0.6 MB** (H.264, 720p, AAC; the OpenH264 encoder works in the image), ZIP, Word and
  Excel to PDF, PDF to Word. A scanned PDF made no Word file, and a GIF was not shrunk.
- **The wheel's ring**: opened for a Word file, a photo, a PDF and a video; labels fit, including WORD (ROUGH) on
  two lines. A real mouse click on ZIP and on SMALLER each made the file and showed a notification.
- **Dolphin's right-click menu** listed Convert to, Remove hidden details, Edit with the edit wheel, Make smaller,
  Trim and Convert with the format wheel.
- **Privacy shield in the top bar**: a left click opened the menu with the six settings and recent events.
- **All nine ArkaOS apps launched from their renamed launchers**, and the dock showed the right icon for each,
  with no placeholder letters. The Bluetooth window opened on a machine with no Bluetooth.
- **Keyboard shortcuts**: 19 of the 24 listed key rows were pressed for real and the result matched the page (Meta+/,
  both wheel keys, launcher, terminal, window overview, peek, tiling, maximise and minimise, spaces, grid, clipboard,
  lock, log-out dialog). Two rows were pressed but not fully checked (Ctrl+F1, Meta+Ctrl+Left). Alt+`, Ctrl+F3 and
  Meta+Ctrl+Shift+arrows were not pressed. Meta+Up from a half-tiled window goes to a corner, so the page now says so.
- **Crash records**: none over two login sessions. The plasmashell crash seen once earlier could not be
  reproduced, so this is not proof that it is fixed.
- **Verified boot**: asked the booted XFS root to enable fs-verity on a file; the kernel answered "Inappropriate
  ioctl for device". XFS does not support it here, so nothing checks file contents by digest at boot.

**Two defects were found in that build, and both are fixed.** The fixes were built (commit `bab62b1`, the five fixes
below) and checked in a fresh VM running that image, the same day:

1. **Time zone search.** Typing "Berlin" now lists Europe/Berlin. Before, it listed nothing.
2. **Blue login screen.** After choosing Blue (Dark look) in setup, the login screen showed a blue avatar ring, a blue
   password field and button, and the blue logo, with the clock on Berlin time.

Also checked in that image: the focus ring on the accent choice is a neutral ring round the dot, not a blue pill; the
shortcuts page lists Ctrl + Alt + T; the Wi-Fi app's title and message say "Wi-Fi"; Alt + `, Ctrl + F3, Ctrl + F1,
Meta + Ctrl + Left and Meta + Ctrl + Shift + Right all did what the page says; and there were no crash records after
setup, a reboot and a second login.

Minor: a notification after Make smaller on a video says "Converted" rather than "Made smaller". Not fixed.

## Not yet checked

- The Light look together with Blue: the Light login theme and the blue copy of it were built but not seen.
- Meta + Ctrl + Shift + Left was not pressed.
- Anything on real hardware, including the Nokia laptop.
- Several monitors, a real touchpad and a real phone.
- Whether the plasmashell crash is gone for good. There were none in these sessions, but it was rare.
