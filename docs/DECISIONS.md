# Architectural Decision Record

Major decisions, their reasoning, the alternatives considered, and status.
Newest at the bottom. If a future change contradicts one of these, update the
status here first — that's the point of the file.

---

**001 · Fedora bootc as the base**
Reason: atomic image-based updates with rollback; the OS is a reproducible
Containerfile; freshest kernel/systemd/desktop stack with first-class bootc
support. Alternatives: Arch (no immutable story), Debian (stale for desktop),
NixOS (different paradigm, steep contributor curve), CentOS Stream 10 (tried
first — missing fs-verity kernel config, stale desktop packages).
**Status: Accepted** (13a switched stream10 → fedora-bootc:42).

**002 · No custom kernel**
Reason: GrapheneOS hardens Linux, it doesn't rewrite it. A from-scratch kernel
is a project-killer; the value is in the integration and experience layers.
**Status: Rejected permanently** unless the project has a team and years.

**003 · KDE Plasma over the custom Hyprland shell**
Reason: we built the Hyprland shell (phases 5–12) and hit the compositor long
tail — window management, session, lock, polkit, notifications, screen share.
Plasma provides a decade of edge cases, themable to the pixel; identity comes
from the design system + our apps. "ArkaWM" = branded KWin.
Alternatives: keep Hyprland shell (drowning in plumbing), write a compositor
(re-enters the same pain deeper). **Status: Accepted** (2026-06-27 pivot).

**004 · GTK4/libadwaita for first-party apps**
Reason: apps predate the KDE pivot, are mature, and libadwaita re-skins
cleanly via named-color overrides to the exact tokens. Rewriting in Qt buys
widget-metric consistency but costs weeks. **Status: Accepted for DP1**;
GTK↔Qt consistency gap is a known issue, revisit after real-user feedback.

**005 · arkad: plain Rust loop, no tokio, CLI-tool enforcement**
Reason: a 60-second verify loop needs no async runtime; enforcing via
well-known tools (nmcli, hostnamectl, sysctl, resolved drop-ins) is auditable
and survives platform updates. Static musl build keeps it dependency-free.
**Status: Accepted.**

**006 · bubblewrap sandbox, not Flatpak**
Reason: no runtime dependency, small image, headless-verifiable, and the
wrapper is baked read-only by composefs. Flatpak remains available *inside*
the OS for user apps (Capsule). **Status: Accepted.**

**007 · bootc-image-builder for all artifacts**
Reason: one source image → qcow2/raw/iso; deployment logic stays in bootc,
not in our scripts. **Status: Accepted.**

**008 · No installer in DP1; thin installer in DP2**
Reason: feature freeze; DP1 audience is developers on QEMU. The DP2 installer
is a front-end over `bootc install to-disk` — no custom partitioning engine,
no OS internals knowledge, no plumbing vocabulary in the UI.
**Status: Accepted** (see INSTALLATION-ARCHITECTURE.md).

**009 · Design tokens enforced at two layers**
Reason: GTK apps read `theme.rs`; Qt/Plasma reads `ArkaOS.colors` +
kdeglobals. The spec (`arka-design-system/`) is canonical; both layers derive
from it. One token change = spec first, then both implementations.
**Status: Accepted.**

**010 · Monorepo until DP1 ships**
Reason: the Containerfile references every path; splitting mid-stabilization
adds risk with zero user value. Post-DP1: GitHub org + `git filter-repo`
split with history. **Status: Accepted, revisit at DP1 release.**

**011 · Feature freeze at DP1 candidate**
Reason: identity is complete; the remaining value is reliability, performance,
consistency. Exit criteria (DP1-EXIT-CRITERIA.md) define done — nothing else
enters. **Status: Active.**

**012 · Deterministic gated build pipeline**
Reason: host-side watchers lied (self-matching pgrep); the pipeline now runs
as one chained command inside the build machine with explicit gates
(BUILD_OK → FIX_PRESENT → BIB_OK) and a build manifest tracing artifact →
commit. **Status: Accepted** (after losing a 4h build to a prune between
stages — see BUILDING.md hard-won rules).
