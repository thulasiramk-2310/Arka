# ArkaOS — public website

`index.html` is the public landing page for ArkaOS: a single, self-contained
file (no build step, no external JS; fonts from Google Fonts). Open it directly
or host it anywhere static.

**One job:** explain ArkaOS to a first-time visitor, honestly. The copy is held
to the same bar as the rest of the project — *honesty over marketing*. It claims
only what a booted image actually does, and labels the dashboard as a UI mockup,
not a security score.

## Deploying (GitHub Pages)

Deployment is automated by [`.github/workflows/pages.yml`](../.github/workflows/pages.yml),
which publishes the `website/` folder on every push to `master`.

**One-time setup** (repo owner, in the browser):

1. GitHub → repo **Settings → Pages**.
2. **Build and deployment → Source:** select **GitHub Actions**.

That's it. After the next push the site goes live at:

```
https://thulasiramk-2310.github.io/Arka/
```

Re-run manually any time from the **Actions** tab → *Deploy website to GitHub
Pages* → *Run workflow*.

## Links

- **GitHub** (nav + footer) → the repository.
- **Read the architecture** → in-page `#architecture` section.
- **Docs** (footer) → `docs/` on GitHub.
- **Build DP1 (qcow2)** → `docs/BUILDING.md`. DP1 is built from source (no
  installer, by design). The built qcow2 is ~4 GB — over GitHub's 2 GB
  release-asset limit — so it is *not* a release download. To offer a hosted
  image later, compress it and host it externally (or split it), then repoint
  this button.

## Design notes

- **Light theme by default**, dark on a toggle (☾ / ☀) — the choice is
  remembered in `localStorage`; with no stored choice the page follows the OS.
- Accent is a warm amber "ray" — *arka* is Sanskrit for *ray of light*.
- Type: **Fraunces** (display) + **IBM Plex Sans / Mono** (body / technical).

---

## Repository discovery metadata

For GitHub repo settings — **Settings → General** (bio) and the **⚙ Topics**
control on the repo home. Kept accurate: no `go`, `d-bus`, or `flatpak` tags,
because those aren't what ArkaOS is built from.

### Bio (< 160 characters)

```
Privacy-first, immutable desktop Linux. One bootc image — read-only composefs root, atomic rollback, and a Rust daemon that defends you by default.
```

### Topics (20)

```
linux
immutable-os
bootc
composefs
privacy
desktop-linux
rust
kde-plasma
wayland
bubblewrap
sandboxing
immutable-infrastructure
dns-over-tls
mac-randomization
atomic-updates
privacy-by-default
fedora-bootc
image-based-linux
tpm
linux-distribution
```

### Search-friendly phrases (weave into descriptions / meta, don't spam)

- image-based / immutable Linux desktop
- atomic upgrades with built-in rollback
- read-only root filesystem (composefs)
- system-level network privacy: encrypted DNS (DoT), MAC randomization
- per-app browser sandboxing with bubblewrap
- "the GrapheneOS-for-desktop gap"
