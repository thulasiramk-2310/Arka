# Future Considerations

These are observations and architectural considerations — a parking lot so ideas
aren't lost.

**Nothing in this document is committed to any release.** The scope of future
releases is determined independently, from real daily-driving experience (see
`docs/FIELD-NOTES.md`), not from this list. Presence here is not a promise.

> DP1 answered *design*. The next unknowns are *hardware* and *deployment*.
> The architecture is no longer the experiment — the hardware is.

The gaps below are **product and ecosystem maturity, not architectural flaws.**
That distinction matters: architectural flaws are expensive to fix later;
product maturity improves with testing, feedback, and time.

Readiness, split honestly: **Engineering ~9/10, Product ~4/10.** The engineering
is mature for a DP1; the uncertainty is deployment and hardware.

---

## The dependency chain (why order matters)

The high-priority items are not peers — they form a chain, and everything assumes
the machine actually works:

```
Real hardware → drivers → power → suspend → installer → full-disk encryption
```

So the first unknown is not the installer. It is **hardware**.

## Hardware

The single biggest unknown. A VM answers none of this:

- CPUs/power: Intel + AMD desktop and laptop; AMD Ryzen power states.
- GPUs: NVIDIA (incl. suspend), AMD, Intel iGPU.
- Wireless: Intel AX and other WiFi chipsets; Bluetooth adapters.
- Input/peripherals: touchpad + gestures, fingerprint reader, webcam.
- Audio codecs; HDMI; USB-C docks; Thunderbolt.
- Displays: HiDPI, multi-monitor.
- Lifecycle: suspend/resume.

First concrete step of the next phase: **boot ArkaOS on one real laptop and see
what breaks.**

## Compatibility

Before 1.0, publish a validated-hardware page — a `docs/HARDWARE.md` or a website
page — listing machines that have been tested, plus known issues. Examples of the
shape (not claims):

```
Validated hardware
  ✓ Lenovo ThinkPad T14 Gen 4
  ✓ Framework Laptop 13
  ✓ ASUS Zephyrus G14
  ✓ Dell XPS 13
  ✓ Intel NUC
  ✓ Ryzen desktop
Known issues
  …
```

This page becomes one of the most valuable things in the project.

## Security

- **Full-disk encryption — required before 1.0.** TPM-sealed LUKS. This is not
  "another security feature"; it is *the defining privacy milestone*. A
  privacy-by-default OS where a stolen laptop's SSD reads clean contradicts its
  own promise. Partly architectural (install-time), so worth *designing* early
  even under the feature freeze — needs real TPM hardware to prove.
- Secure Boot chain, production story:
  `firmware → Secure Boot → signed bootloader → signed kernel → measured boot → TPM`.
  Measured-boot work exists but PCR 11–15 are blocked on platform bits only
  shakeable on real hardware (see `PHASE3-FINDINGS.md`).
- Recovery: recovery mode, rollback UI, repair/safe mode — "desktop broken, how
  does the user recover?"
- Telemetry policy: formalize — no analytics; optional crash reports; a
  data-handling statement.

## Install

- Live ISO + thin installer (`download → flash USB → install → use`). Downstream
  of hardware validation — an installer for unvalidated hardware installs a
  brick. DP1 has no installer *by design* (developer audience).

## Sandboxing / app isolation

- **Do not generalize the bespoke bubblewrap wrapper to all apps** — that is a
  "maintain-forever" trap that rebuilds what Flatpak portals already provide.
  The Firefox wrapper stays because it is part of the product identity; every
  other third-party app should ride Flatpak + portals, which the ecosystem
  maintains.

## Ecosystem

- Not an Arka store. Capsule = curated Flathub: verified apps, update status,
  trust indicators — layered on Flathub's AppStream metadata, not a new registry.

## UX / product

- Update UX: the bootc mechanism exists; polish the messaging —
  `Update Ready · Restart Tonight / Restart Now / Remind Me Later`.
- User accounts: password change, multiple users, guest mode, optional parental
  controls.
- Settings state accuracy (e.g. the Automatic Login toggle reflecting the real
  SDDM state — see `docs/FIELD-NOTES.md`).
- Backup: local + external-drive backup, restore wizard.

## Performance

- Measure continuously and track regressions over time: boot time, idle RAM,
  idle CPU, app launch latency.

## Accessibility

- Needs a proper audit: screen readers, keyboard navigation, large text,
  contrast, color blindness, focus visibility. Often overlooked; easier to build
  in than bolt on.

## Internationalization

- Localization, RTL languages, Unicode coverage, regional formats. English-only
  today.

## Architecture — the long-term shape

The single most valuable non-feature investment, and the one place where
deferring is genuinely expensive later:

**Every Arka app talks to Arka *service interfaces*, never KDE/Plasma APIs
directly.** `WindowService` is the template. Each service is an Arka interface
with a swappable implementation:

```
PowerService  → PowerDevil   (today)  →  Arka Power   (tomorrow)   — no UI change
WindowService → KWin         (today)  →  ArkaWM       (tomorrow)   — no UI change
```

Candidate services: `WindowService` (exists), `PowerService`, `NetworkService`,
`NotificationService`, `PermissionService`, `PrivacyService`, `UpdateService`,
`ThemeService`. None of them name KDE. This is what keeps the "swap Plasma /
bring ArkaWM" door open without rewriting the desktop apps — so defining the
trait first (even when KDE is the only implementation) is the discipline worth
keeping during the freeze.

## Guardrails (the don't-do list)

- **Don't reimplement mature Linux infrastructure** — own package manager,
  network stack, filesystem, kernel — without a compelling reason. ArkaOS's value
  is the *experience*, not novel plumbing.
- **Don't try to be everything at once** — distro + DE + WM + package ecosystem +
  mobile + car. That list has killed better-funded projects. For the next few
  years: build the best desktop operating-system experience possible. Everything
  else can come later.
