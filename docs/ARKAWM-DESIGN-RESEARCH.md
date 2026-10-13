# Arka desktop design research

Status: research notes, 2026-10-12. Nothing here is built. The evidence is
qualitative (reviews, forum threads, blog posts), so read "people praise" as
"some people say", not as a measured ranking. No survey I found compares
macOS, Windows, GNOME and KDE on what users like.

The goal from the user: a desktop that combines the best-loved ideas from
other systems and still looks and feels like ArkaOS.

## What people praise, by system

| System | Praised idea | Source |
|---|---|---|
| GNOME | One key (Super) shows every window; empty workspaces appear as needed; touchpad gestures; an uncluttered top bar | [opensource.com](https://opensource.com/article/18/8/what-i-love-about-gnome), [The New Stack](https://thenewstack.io/what-makes-gnome-so-appealing/) |
| KDE Plasma | Almost everything can be changed, and the defaults work without touching them | [opensource.com](https://opensource.com/article/22/2/why-i-love-linux-kde), [The New Stack](https://thenewstack.io/linux-desktop-what-makes-kde-plasma-so-appealing/) |
| Windows 11 | Snap Layouts: hover the maximize button or press Win+Z, no manual resizing. TechRadar says many people never use it | [PC Gamer](https://www.pcgamer.com/uk/snap-layouts-are-windows-11s-best-new-feature), [TechRadar](https://www.techradar.com/news/microsoft-wants-more-people-to-use-this-unloved-windows-11-feature) |
| macOS | Spotlight search, Mission Control overview, Stage Manager (opinions split), per-app prompts for camera and microphone | [Softpedia](https://www.softpedia.com/reviews/mac/macos-ventura-review-536588.shtml), [LWN comment](https://lwn.net/Articles/813992) |
| niri | Windows sit in an endless row; opening one never resizes the others; separate workspaces per monitor | [LWN](https://lwn.net/Articles/1025866), [It's FOSS](https://itsfoss.com/niri-window-manager/) |
| Android, iOS | The camera and microphone indicator that names the app using it | [LWN thread](https://lwn.net/Articles/813992) |

Complaints worth noting: GNOME's default workflow feels limiting to some and
leans on extensions; Stage Manager confuses people used to Mission Control;
snapping goes unused when people don't find it.

## What no desktop has, and ArkaOS can

Linux has no standard equivalent of the phone's camera and microphone dot.
GNOME's per-app permission lists fill in only for Flatpak apps, and the
indicator I found is an unreviewed extension ([Arch forum](https://bbs.archlinux.org/viewtopic.php?id=265942),
[Privacy Guides](https://discuss.privacyguides.net/t/complete-lack-of-per-app-file-access-and-permissions-control-in-linux-and-also-other-destops-os-like-mac-and-windows/22392),
[GNOME extension](https://extensions.gnome.org/review/74524)). That gap fits
ArkaOS's positioning, so the unique part should be privacy you can see:

1. Privacy dots drawn by the shell. Microphone, camera and screen-capture
   dots that name the app. The goal is that no app window can draw over or fake
   them. That is a design goal to verify, not a claim to publish yet.
2. Sandbox state on the window. A small mark on a window's title area when
   the app runs sandboxed (our Firefox does) and what it can see.
3. Update and recovery in the shell. ArkaOS can go back to the previous
   version; the shell can show "healthy, previous version ready" in one place.

Every one of these must claim only what ArkaOS can prove, as in the rest of the
product.

## The mix to build

- Super overview: windows and search together (GNOME overview with a
  Spotlight-style search box).
- Snap layouts: Windows-style hover zones and a keyboard shortcut, plus an
  optional scrolling row per workspace for people who like niri.
- Workspaces: appear as needed, one set per monitor.
- Touchpad gestures: the Nokia's touchpad already works.
- KDE-level choice: everything changeable, sensible defaults, our accent
  only on ArkaOS's own surfaces.
- The three privacy ideas above, which are what make it recognisably ArkaOS.

Visual rule stays as set: premium and calm, light first with a Light/Dark
choice, no glow or glass.

## KWin or ArkaWM

Most of this list does not need a new compositor. KWin takes scripts and
effects, and the Arka Shell plan already covers panels and the launcher. A
compositor of our own only pays off for what KWin cannot express, most likely a
scrolling row and shell-drawn privacy dots that apps cannot cover.

[Smithay](https://github.com/Smithay/smithay) is a toolkit, not a finished
compositor. niri and COSMIC are built on it, so the route is proven. Its latest
release I could see is 0.7.0 (docs.rs, June 2025); check crates.io before
depending on it.

Proposed order:

1. Prototype overview, snap layouts and privacy dots on KWin and Plasma first,
   and note which ones hit a limit.
2. ArkaWM spike, research only: a Smithay compositor in a nested window on the
   dev laptop (Smithay's own `anvil` example is the reference), one window, a
   scrolling row, one privacy dot. Time-box it.
3. ArkaWM replaces KWin only when it matches KWin on accessibility, portals,
   screencast, multi-monitor, scaling and XWayland.
