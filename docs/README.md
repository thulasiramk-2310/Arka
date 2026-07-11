# ArkaOS documentation

Facts, plans, and ideas are kept **separate** here, and every document has
exactly one job. Nothing slowly becomes "the roadmap."

## The map

| Document | Its one job |
|----------|-------------|
| [`../HISTORY.md`](../HISTORY.md) | **What happened** — the story and its turning points |
| [`DECISIONS.md`](DECISIONS.md) | **Why** it happened — architectural decision records |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | **How it works** — the component map and reasoning |
| [`THREAT-MODEL.md`](THREAT-MODEL.md) | **What we defend against** — and what we don't |
| [`INSTALLATION-ARCHITECTURE.md`](INSTALLATION-ARCHITECTURE.md) | **How it gets onto disk** — delivery strategy |
| [`BUILDING.md`](BUILDING.md) | **How to build it** — source → image → VM |
| [`TESTING.md`](TESTING.md) | **How we verify** — doctrine, QA harnesses, budgets |
| [`DP1-EXIT-CRITERIA.md`](DP1-EXIT-CRITERIA.md) | **When we shipped DP1** — the gate it had to pass |
| [`RELEASE-NOTES-DP1.md`](RELEASE-NOTES-DP1.md) | **What's in DP1** — and its honest known issues |
| [`../CHANGELOG.md`](../CHANGELOG.md) | **What changed** — release by release |
| [`FIELD-NOTES.md`](FIELD-NOTES.md) | **What using it feels like** — the experiential lens |
| [`DP2.md`](DP2.md) | **Facts already merged** since DP1 (queued for the next build) |
| [`FUTURE-CONSIDERATIONS.md`](FUTURE-CONSIDERATIONS.md) | **Ideas** — candidate work, committed to nothing |
| [`../arka-design-system/README.md`](../arka-design-system/README.md) | **The design system** — tokens, motion, components |
| [`../legacy/README.md`](../legacy/README.md) | **The archive** — the retired Hyprland-era shell |

Deep-dive findings from earlier phases live at the repo root:
[`../PHASE3-FINDINGS.md`](../PHASE3-FINDINGS.md) (integrity & boot hardening),
[`../PHASE4-SANDBOX.md`](../PHASE4-SANDBOX.md) (browser sandbox proof).

## How an idea becomes a release

The point of the separation is a pipeline, not a pile. Ideas don't jump
straight into a release; they earn their way there through real use:

```
idea → FUTURE-CONSIDERATIONS.md → daily use → FIELD-NOTES.md
     → a pattern (it recurs) → DECISIONS.md → DP2.md (merged) → release
```

An idea that appears once in `FIELD-NOTES.md` is an observation. One that
appears repeatedly is a candidate. Only candidates that survive real use — and
get a decision recorded — become merged facts in `DP2.md` and ship. This is how
the project stays focused: **release scope is shaped by experience, not by
speculation.**
