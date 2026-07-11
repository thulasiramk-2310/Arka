# legacy/ — Hyprland-era shell (archived)

These components are from the **Hyprland prototype phase** of ArkaOS and are
retained for historical reference. They are **no longer part of the active
ArkaOS desktop** and are not built into or shipped in the OS image.

| Crate | Was | Replaced in the KDE Plasma desktop by |
|-------|-----|----------------------------------------|
| `arka-bar` | custom top bar (a waybar replacement) | Plasma panel |
| `arka-dock` | floating app dock | Plasma `icontasks` (floating panel) |
| `arka-launcher` | full-screen app launcher | Plasma `kickerdash` (Application Dashboard) |

## Why they're here and not deleted

The KDE Plasma pivot (2026-06-27) replaced this custom shell wholesale. The
binaries stopped being autostarted and stopped being shipped, but the source is
kept for a release or two so the project's history stays legible — you can see
what the Hyprland-era shell looked like and how it worked. Once it's clear
nothing needs them, they can be removed for good.

## Notes

- Built on `gtk4-layer-shell` (the wlroots layer-shell protocol) — the reason
  they belonged to the Hyprland/sway era. The active `arka-shell` apps no longer
  depend on it.
- This is a **separate Cargo workspace** (`legacy/Cargo.toml`), isolated from the
  active `arka-shell/` workspace. They still path-depend on the live
  `arka-shell-common` crate (`../../arka-shell/arka-shell-common`).
- Building them requires the layer-shell toolchain and a wlroots compositor to
  run; they are not expected to function under KWin.

See `docs/DP2.md` for the deprecation decision.
