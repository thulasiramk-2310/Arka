# The story of ArkaOS

Not a changelog — a story. The changelog (`CHANGELOG.md`) records *what*
changed; this file remembers *how the project became what it is*, so that years
from now the path is still legible.

> **Built on Linux. Designed for people.**

---

## 2026 — from experiment to Developer Preview

**The question.** It started as a set of open questions rather than a plan:
Hyprland or something custom? bootc, composefs? Build our own compositor? Our
own kernel? What would a real "GrapheneOS-for-desktop" even look like? Early on,
the ambition ran ahead of the shape.

**The foundation.** The first real decision was the immutable model: a single
bootc image, composefs read-only root, atomic updates with rollback. On top of
that came `arkad` — the privacy daemon that enforces MAC randomization,
encrypted DNS (DoT/Quad9), a generic hostname, and IPv6 privacy addresses, and
re-verifies them every 60 seconds. Then boot hardening (TPM PCR measurement to
PCR 10) and browser isolation (Firefox in a bubblewrap sandbox that can't see
your files).

**The first desktop.** A custom shell arrived on a wlroots compositor —
`arka-bar`, `arka-dock`, `arka-launcher` — the Hyprland prototype. It booted, it
worked, it proved the idea. `v0.1-alpha` was tagged here (2026-06-22).

**The pivot.** On 2026-06-27 came one of the most important calls of the
project: **retire the custom Hyprland shell and build on KDE Plasma instead.**
Not because the prototype failed, but because Plasma better served the actual
goal — a *usable* desktop OS — providing the panel, launcher, settings, file
manager, and window management so the project could focus on what makes ArkaOS
*ArkaOS*. Choosing to pivot rather than defend an earlier technical choice is
what kept the project healthy. (The Hyprland shell now lives in `legacy/`.)

**The identity.** With Plasma as the base came the Arka Design System v1.0
(tokens, typography, motion, components, a privacy color mapping), a full brand
identity — Plymouth boot animation, SDDM login theme, icon pack, cursor, sounds,
a generated signature wallpaper — and the first-party apps: the Privacy
Dashboard (privacy score, event timeline, weekly report), Capsule (a Flathub
app front-end), Settings, and the rest.

**The discipline.** Somewhere in here the work changed character — from feature
coding to release engineering: an architecture doc, ADRs, a threat model, a
testing doctrine (compile success ≠ runtime correctness; every change boots in a
VM first; security claims require proofs against the deployed artifact), DP1
exit criteria, and a repeatable build pipeline.

**Developer Preview 1 — 2026-07-11.** Tagged `v0.1.0-dp1`. Built, boot-verified
(including the two-months-recurring first-boot wizard black-screen bug, finally
root-caused and fixed), and shipped with real screenshots and honest known
issues. The difference between a project and a product.

> Developer Preview 1 wasn't the moment ArkaOS became *usable*. It was the
> moment it became *sustainable* — frozen releases, decision records, a threat
> model, build gates, exit criteria, field notes, factual milestones, a future
> parking lot, and the willingness to change direction when the evidence
> demanded it. Anyone can fork the Rust, the icons, the wallpaper. The
> discipline is the part a fork can't copy overnight.

---

## Milestones

| When | Milestone |
|------|-----------|
| 2026 | Project started — immutable bootc foundation |
| 2026 | First immutable image booted |
| 2026 | `arkad` privacy daemon; browser sandbox; TPM measured boot |
| 2026 | Hyprland prototype shell (`v0.1-alpha`, 2026-06-22) |
| 2026-06-27 | **Pivot to KDE Plasma** — custom shell retired |
| 2026 | Arka Design System v1.0; Privacy Dashboard; Capsule |
| **2026-07-11** | **Developer Preview 1 released — `v0.1.0-dp1`** |

---

*Add to this file at each real turning point — a pivot, a first, a release — not
at every commit. It's the project's memory of its own shape.*
