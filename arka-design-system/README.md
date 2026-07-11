# Arka Design System v1.0

The single source of truth for how ArkaOS looks, moves, and sounds.
Every surface — shell, apps, login, boot, docs — follows this system.
When someone sees a screenshot, they should immediately think: **this is ArkaOS.**

---

## 1. Colors

| Token | Hex | Use |
|---|---|---|
| `bg.base` | `#07080E` | App/desktop background |
| `bg.raised` | `#0F141A` | Cards, panels, dock |
| `bg.overlay` | `#161C24` | Popovers, inputs, tooltips |
| `bg.sunken` | `#050609` | Wells, scrollbar tracks, sidebars |
| `border.sub` | `#161C24` | Row separators |
| `border.ui` | `#1E2630` | Card & input borders |
| `border.emph` | `#2A3340` | Focus-adjacent, hover borders |
| `text.hi` | `#F5F7FA` | Primary text |
| `text.lo` | `#9AA4B2` | Secondary text |
| `text.muted` | `#5B6675` | Placeholders, disabled |
| `accent` | `#16C784` | THE Arka green. Privacy = safe. Focus, toggles-on, links-primary, selection |
| `accent.dim` | `#0E8F5D` | Pressed accent |
| `secondary` | `#2E7BFF` | Informational blue. Links, info badges. Never competes with accent |
| `danger` | `#FF4D4F` | Destructive, denied, errors |
| `warn` | `#F5A623` | Drift, attention |

**Privacy colors:** protected/enforced = `accent`. Exposed/denied-by-user = `danger`.
Drifted/pending = `warn`. Informational = `secondary`. There is no other mapping.

**Rules:** text on accent is `bg.base` (dark on green), never white.
Accent is earned — one accent element per view region. Backgrounds are never pure black.

## 2. Typography

- **UI:** Inter. Body 13px, secondary 12px, meta 10px/600, row-title 15px/600, heading 20px/600, display 26px+/700.
- **Code / technical values:** JetBrains Mono (MACs, IPs, hashes, logs).
- Uppercase micro-labels get +2–3px letter-spacing. No italics in UI.

## 3. Spacing & Grid

8px grid. Components use 8/16/24/32. Inline gaps may use 4.
Cards pad 16, dialogs pad 24, page margins 16–24.

## 4. Corner Radius

| Radius | Use |
|---|---|
| 8 | Buttons, inputs, badges |
| 12 | Cards, rows-grouped, menus |
| 18 | Dialogs, sheets |
| 24 | Dock, launcher panel |
| 999 | Pills, toggles, dots |

## 5. Shadows

One direction only: `0 8px 24px rgba(0,0,0,0.45)` for floating surfaces
(dock, popovers, dialogs). Flat surfaces get borders, not shadows.

## 6. Animation

| Motion | Duration | Easing |
|---|---|---|
| Hover / state change | 120ms | ease-out |
| Open (menus, dialogs) | 180ms | ease-out-cubic |
| Close | 150ms | ease-in |
| Page fade / login fade-in | 650ms | ease-out-cubic |

KWin: wobbly windows (subtle), magic-lamp minimize, scale on open/close.
Nothing bounces twice. Nothing moves without a reason.

## 7. Iconography

Line icons, 3px stroke, round caps/joins, on 64×64 grid.
App icons: rounded square `#0F141A` plate, `#1E2630` 2px border, radius 14,
glyph in `accent`. Ship set lives in `arka-icons/` (dashboard, capsule,
settings, wifi, update, perms, sound, hotkeys, bluetooth, welcome, files, browser).
System icons inherit breeze-dark; replace opportunistically, never mix styles in one row.

## 8. Cursor

`ArkaCursor` — white pointer, green outline, inherits breeze_cursors for
the long tail of shapes.

## 9. Sound

`Arka` freedesktop sound theme. Soft, short (≤0.4s), sine-based, quiet.
Events: login, notification, error, screenshot, device add/remove, battery-low.
No sound for routine actions.

## 10. Wallpaper

Deep `#07080E` field, blue radial glow, faint triangle grid, layered ▲ marks,
`ARKAOS` wordmark in accent + tagline. Generated at build (ImageMagick), 1920×1080.

## 11. Logo Usage

- `▲ ARKA` — shell surfaces (top panel, dock, menus).
- `ARKAOS` — About, installer, boot splash, login, wallpaper only.
- Wordmark always in `accent` on dark, letter-spaced (3–6px).
- Never stretch, recolor, or put the mark on light backgrounds.

## 12. Components

- **Buttons:** radius 8, 46px login/36px UI height. Primary = accent bg + `bg.base` text.
  Secondary = `bg.overlay` + `border.ui`. Destructive = danger.
- **Cards:** `bg.raised`/`bg.overlay`, `border.ui`, radius 12, pad 16.
- **Inputs:** `bg.overlay`, `border.ui` → `accent` on focus, radius 8.
- **Toggles:** 32×18 pill, accent when on.
- **Dialogs:** radius 18, pad 24, one primary action.
- **Notifications:** `bg.overlay` card, radius 12, icon tinted by privacy color.
- **Dock:** floating, radius 24, `bg.raised`, centered, icons only.
- **Top panel:** 36px, `bg.base`, ▲ ARKA left, tray + clock right.
- **Launcher:** full-screen scrim over wallpaper, search-first.
- **Badges:** 10px/600, tinted `alpha(accent,0.10)` + accent text.
- **Charts:** accent for the hero series, `secondary` for comparison, `text.muted` axes.
  Privacy stats always green-positive (more blocked = more green).

## 13. Boot & Login

- **Plymouth:** three staggered ▲ fade in, outer glow pulses, `ARKA` wordmark
  rises, thin accent progress line. ≤5s. Theme in `plymouth-theme-arkaos/`.
- **SDDM:** `sddm-theme-arkaos/` — wallpaper under scrim, clock top-right,
  centered wordmark + tagline, single password field, accent Unlock.

## Implementation map

| Layer | File |
|---|---|
| GTK shell tokens | `arka-shell/arka-shell-common/src/theme.rs` |
| Plasma/Qt scheme | `ArkaOS.colors` + `/etc/skel/.config/kdeglobals` |
| Icons | `arka-icons/` → `/usr/share/icons/Arka` |
| Cursor | built in Containerfile → `/usr/share/icons/ArkaCursor` |
| Sounds | built in Containerfile → `/usr/share/sounds/Arka` |
| Boot splash | `plymouth-theme-arkaos/` |
| Login | `sddm-theme-arkaos/` |
| Wallpaper | Containerfile ImageMagick stage |

A change to a token happens here first, then propagates to every layer above.
