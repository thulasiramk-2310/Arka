# DP1 Exit Criteria

DP1 ships when every box is checked — and then it ships. No new features enter
this list. Each section is verified in a booted VM built by the gated pipeline
(BUILD_OK → FIX_PRESENT → BIB_OK), not from the working tree.

## Build
- [ ] Container builds clean (`BUILD_OK`)
- [ ] Image content verified (`FIX_PRESENT` gate)
- [ ] Disk image produced (`BIB_OK`)
- [ ] `build-manifest.json` generated; artifact sha256 recorded

## Boot
- [ ] Plymouth splash (triangles → ARKA → progress line)
- [ ] SDDM login (arkaos theme)
- [ ] OOBE welcome wizard (first boot only, never again)
- [ ] Desktop reaches idle
- [ ] `systemctl --failed` is empty
- [ ] arkad active and enforcing

## Desktop
- [ ] Wallpaper · icons · cursor · Inter fonts · ArkaOS color scheme
- [ ] Top bar (36px) and floating dock present after first login

## Core apps (launch + basic function)
- [ ] Launcher · Dashboard · Capsule · Settings · WiFi · Update · Permissions

## Privacy
- [ ] Privacy Score reported over D-Bus
- [ ] Browser sandbox proof passes (sentinel unreadable, /etc allowlisted)
- [ ] DoT active (resolvectl) · MAC randomization conf present · hostname `arka`
- [ ] Weekly report renders on Dashboard

## UX
- [ ] Notifications appear styled
- [ ] Sound theme events audible (or files verified present + configured in VM)
- [ ] Keyboard + mouse navigation through all shipped surfaces

## Documentation
- [ ] README (website-grade) · Architecture · Building · Testing ·
      Installation · Design System · Release Notes · Threat Model · DECISIONS

## QA
- [ ] Boot loop: 100/100 clean (qa-boot-loop.sh)
- [ ] Memory budgets met (arkad ≤ 30 MB · desktop idle ≤ 2.5 GB)
- [ ] Boot time ≤ 20 s in VM
- [ ] Known issues documented in CHANGELOG

## Release
- [ ] Staged commits pushed (no co-author lines, 1–2 line messages)
- [ ] Annotated tag `v0.1.0-dp1-candidate` → QA → `v0.1.0-dp1`
- [ ] Release notes final · screenshots in README · manifest + checksums

Then **stop**. Week of daily use → friction notes into DP2.md → only bugs and
polish until DP2 planning opens.
