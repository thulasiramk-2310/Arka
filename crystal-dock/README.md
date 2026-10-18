# ArkaOS Dock (Crystal Dock settings)

The dock is [Crystal Dock](https://github.com/dangvd/crystal-dock), started at
login by `arkaos-dock.desktop`. Plasma panels can't draw outside
their own strip, so a Plasma widget can't grow icons above the dock the way the
macOS Dock does; Crystal Dock is its own Wayland layer-shell window and can.

New accounts get these settings in `/etc/skel/.config/crystal-dock/KDE/`, and a
copy in `generic/`, which Crystal Dock reads when `XDG_CURRENT_DESKTOP` is unset
(without it, its first-run window appears with the Version Checker ticked):

- `panel_1.conf`: Settings, Discover, Files and Firefox, the application menu
  and Trash; Intelligent Auto Hide (the dock gets out of the way only when a
  window covers it). The Version Checker, which asks api.github.com for new
  releases, is off.
- `appearance.conf`: the Flat 2D style, a small dot under running apps,
  icons from 48 px growing to 80 px under the pointer. These are the dark
  colours; `arka-apply-look` switches the background, border and dots with
  the Light/Dark look.

The image builds Fedora's own crystal-dock source package with
`arkaos-look.patch` (`build-rpm.sh`, in the Containerfile's dock-builder
stage). For the Flat 2D style the patch:

- asks KWin to blur what is behind the dock, inside its rounded background
  only (KWin skips blur on software rendering, as in a VM without 3D);
- draws the app name in a small rounded label instead of large outlined text;
- adds a divider before Trash, rounds the corners more and draws a 1 px
  border in `borderColor`.

If Fedora updates crystal-dock and the patch no longer applies, the image
build stops instead of shipping the dock without it.

Every setting stays changeable from the dock's right-click menu.
