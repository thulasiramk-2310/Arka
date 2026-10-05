# Contributing to ArkaOS

Thanks for helping. ArkaOS is a developer preview, so the most useful
contributions right now are **testing on real hardware**, bug reports, and small,
focused fixes.

## Reporting bugs

Use the issue form. Tell us whether you ran ArkaOS in a virtual machine or on a
real computer (make and model), the version (`cat /usr/lib/arkaos-release`), and
what you expected to happen. Security problems go through [SECURITY.md](SECURITY.md),
not public issues.

## Building

The whole OS is one container image. See [docs/BUILDING.md](docs/BUILDING.md).

```sh
podman build -t localhost/arkaos:dev .
scripts/make-disk.sh output/disk.raw 20G   # bootable disk image
```

## Before you open a pull request

1. **Run the source checks** for what you touched, for example
   `cargo test` in `arkad/` or `arka-shell/`, and `python3 scripts/test-oobe-apply.py`.
2. **Boot it.** A change that compiles isn't done: build the image and boot it
   in a VM (see [docs/TESTING.md](docs/TESTING.md)). Say in the pull request what
   you checked.
3. **Check every new sentence a user will read.** ArkaOS only claims what it can
   show. Say what a setting *does*, never that someone "is protected" or "can't
   be tracked". No scores or percentages for privacy.
4. **Keep it small.** One topic per pull request and per commit.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/), subject line only:

```
fix(dashboard): show when a check is out of date
feat(arkad): check the firewall zone
docs(readme): describe the Fedora 44 candidate
```

## Design

ArkaOS builds on Fedora bootc, systemd and KDE Plasma instead of replacing them.
Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/DECISIONS.md](docs/DECISIONS.md) before proposing larger changes.

## Code of conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
