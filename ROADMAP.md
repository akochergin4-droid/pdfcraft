# PdfCraft roadmap

> **Last reviewed:** 2026-11-03 | **Last updated:** 2026-11-03 | **Change:** patch (M5 #5850 review: indirect typewriter intent and composer edits preserve outer whitespace; movement regression commits by outside click; estimates unchanged)

**Stage: alpha** · next: beta, ~25% (ready for real work 50% → 75%) and ~400–700 h away

PdfCraft's one-page summary: how close it is to Adobe Acrobat Pro's offline feature set, and what
comes next. Adobe's cloud services (Document Cloud, Acrobat Sign requests, shared review, the AI
Assistant) are out of scope; PdfCraft's alternatives are local-first plus opt-in providers.

| Document | Holds |
|---|---|
| [docs/target-app-parity.md](docs/target-app-parity.md) | The authoritative assessment: method, weights, every dimension and area, stage, calibration |
| [docs/gaps.md](docs/gaps.md) | Every known shortfall, ranked: the work list |
| [docs/roadmap.md](docs/roadmap.md) | Milestones M0–M14, current focus, upcoming milestones, risks |
| [docs/pdf-spec-parity.md](docs/pdf-spec-parity.md) | ISO 32000-2 coverage, annotations, actions, signatures, PDF/A/X/UA |
| [docs/file-format-parity.md](docs/file-format-parity.md) | Every format Acrobat reads or writes, create/export conversions, OCR |
| [docs/ui-parity.md](docs/ui-parity.md) | Tools, handles, shortcuts, panels, preferences |
| [docs/hardware-parity.md](docs/hardware-parity.md) | GPU, printers, scanners, smart cards, pen, platforms |
| [docs/localization-parity.md](docs/localization-parity.md) | Per-language status |
| [docs/architecture.md](docs/architecture.md) | How PdfCraft is built |
| `parity/acrobat-features.toml` | The machine-checked checklist (`cargo xtask parity`) |

**Keep it current:** every session, update the milestone row in [docs/roadmap.md](docs/roadmap.md)
and add a line to the [progress log](docs/roadmap-history.md#progress-log); when a gap closes, remove it from
[docs/gaps.md](docs/gaps.md); re-measure fully when a major arc lands, at least monthly.

## Headline numbers

| | Value | Kind |
|---|---|---|
| **Feature breadth** | **≈ 67%** tier-weighted (57.0% unweighted: 423 shipped + 75 partial of 808; P0 94.2%) | measured, `parity/acrobat-features.toml` |
| Feature breadth, mainstream practitioner | ≈ 70% tier-weighted (63.3% unweighted; P0 95.3%) | measured ([split](docs/target-app-parity.md#mainstream-practitioner-vs-full-acrobat-pro)) |
| **Ready for real work**, full Acrobat Pro | **≈ 50%** (45–53%) | estimated, weighted sum over [dimensions](#by-dimension) |
| **Ready for real work**, mainstream practitioner | **≈ 55%** (50–60%) | estimated ([split](docs/target-app-parity.md#mainstream-practitioner-vs-full-acrobat-pro)) |
| **Ready for real work**, essentials user | **≈ 66%** (60–70%) | estimated ([method](docs/target-app-parity.md#essentials-user)) |
| Remaining to **beta** | ≈ 400–700 Opus 5.5 agent-hours | estimated |
| Remaining to **full parity** | ≈ 780–1,400 Opus 5.5 agent-hours (≈ 300–550 h wall clock with 3–5 agents) | estimated, calibrated on this repo's history |

**Why alpha:** all six of Acrobat Pro's everyday workflows can be completed end to end on macOS
(the [alpha gate](docs/roadmap.md#alpha-gate)), and users praise them (7.6k stars, ≈ 217k downloads
of 0.4–0.5, switching reports such as #179). Two are only partial: editing (no CJK/RTL) and convert
and share (no in-app Office→PDF; Word export loses layout, #773; password-protected files open in
Acrobat since #836, #774). Those, unmeasured quality against Acrobat, the borrowed `hayro` renderer,
and 245 open issues keep it far from beta.

### Readiness by audience

| Audience | Ready | Opus 5.5 agent-hours to ≈ 95% | Work that dominates |
|---|---|---|---|
| Full Acrobat Pro (ready for real work) | ≈ 50% | ≈ 780–1,400 h (≈ 300–550 h wall clock with 3–5 agents; 60–70% parallelizes) | Own renderer and fidelity harness, editing existing content, OCR and Office conversion, Preflight and PDF/A/X/UA, accessibility tagging, signatures for institutions |
| Mainstream practitioner | ≈ 55% | ≈ 500–900 h (≈ 200–400 h wall clock; ≈ 60% parallelizes) | Rendering fidelity, editing existing text (CJK, RTL, subsets), conversion (Word layout, Excel, PowerPoint, Office→PDF), OCR languages, the bug backlog |
| Essentials user | ≈ 66% | ≈ 180–330 h (≈ 90–180 h wall clock; ≈ 50% parallelizes) | Rendering fidelity on everyday files, launch and GPU stability, simple text fixes, Word export layout, Chinese UI glyphs |

Hours are calibrated as in [Effort and calibration](docs/target-app-parity.md#effort-and-calibration) (≈ 1k kept lines per agent-hour for new features, 3–10× slower for hardening and fidelity). The audiences are nested subsets, so essentials ≤ mainstream ≤ full.

## By dimension

| Dimension | Ready | Weight | Remaining (h) | Doc |
|---|---|---|---|---|
| Features (14 areas) | ≈ 49% depth · 67% breadth | 40% | 610–1,100 | [target-app-parity.md](docs/target-app-parity.md#by-feature-area) |
| UI/UX fidelity | ≈ 55% | 10% | 30–60 | [ui-parity.md](docs/ui-parity.md) |
| File formats and conversion | ≈ 40% | 12% | 120–220 (within features) | [file-format-parity.md](docs/file-format-parity.md) |
| PDF specification and standards | ≈ 60% | 8% | 150–260 (within features) | [pdf-spec-parity.md](docs/pdf-spec-parity.md) |
| Hardware | ≈ 35% | 4% | 25–50 | [hardware-parity.md](docs/hardware-parity.md) |
| Localization | ≈ 68% | 5% | 35–60 | [localization-parity.md](docs/localization-parity.md) |
| Performance | ≈ 40% | 6% | 25–45 | [gaps.md](docs/gaps.md) gap 13 |
| Stability | ≈ 50% | 8% | 45–80 | [gaps.md](docs/gaps.md) gap 6 |
| Platforms | ≈ 70% | 5% | 15–30 | [hardware-parity.md](docs/hardware-parity.md#platforms) |
| Ecosystem and plug-ins | ≈ 30% | 1% | 10–20 (within features) | [gaps.md](docs/gaps.md) |
| AI features | ≈ 0% (in scope: local providers) | 1% | 20–40 (within features) | [gaps.md](docs/gaps.md) gap 23 |
| **Total (weighted sum)** | **≈ 50%** | 100% | **≈ 780–1,400** | |

## Features

| Area | Breadth (measured) | Ready (estimated) | Remaining (h) |
|---|---|---|---|
| A Core model and fidelity | 76.8% | 70% | 25–45 |
| B View and navigation | 62.5% | 55% | 110–190 |
| C Content editing | 53.3% | 30% | 100–180 |
| D Organize pages | 61.2% | 70% | 12–25 |
| E Comments and review | 77.7% | 70% | 12–25 |
| F Forms and JavaScript | 78.5% | 65% | 40–75 |
| G Protect, redact, sanitize | 58.5% | 55% | 15–30 |
| H Digital signatures | 62.2% | 45% | 40–70 |
| I Scan and OCR | 23.1% | 15% | 40–75 |
| J Create and export | 39.7% | 25% | 55–100 |
| K Optimize and standards | 35.3% | 20% | 65–115 |
| L Print | 39.7% | 35% | 30–55 |
| M Accessibility | 63.1% | 30% | 40–70 |
| N Automation, preferences, misc | 30.0% | 35% | 25–45 |

What's strong and missing in each: [target-app-parity.md](docs/target-app-parity.md#by-feature-area).

## Languages

Twelve key languages (historical % snapshot vs 2,143 keys, except Vietnamese, Brazilian
Portuguese and Spanish vs the current union). Detail and script notes: [localization-parity.md](docs/localization-parity.md).

| Language | Code | Strings | Status |
|---|---|---|---|
| English | `en` | 100% | full |
| Simplified Chinese | `zh-hans` | 97.4% | partial (native LXGW WenKai Lite; no Hans face in the web build, #826) |
| Spanish | `es` | 100% of current union | partial (dialogs / tooltips; no help) |
| Hindi | `hi` | 0% | none |
| Arabic | `ar` | 100% | partial (logical catalog, explicit RTL labels, mirrored stock chrome; native/browser/IME workflows unqualified) |
| French | `fr` | 99.3% | partial |
| Portuguese (Brazil) | `pt-br` | 100% of current union | partial |
| Indonesian | `id` | 0% | none |
| Japanese | `ja` | 99.1% | partial |
| German | `de` | 99.3% | partial |
| Korean | `ko` | 0% | none |
| Vietnamese | `vi` | 100% of current union | partial (dialogs / tooltips; no help) |

Also shipped: Traditional Chinese, Russian, Bulgarian, Telugu, Hungarian, Ukrainian, Italian,
Kazakh, Polish, Slovenian, Greek, Turkish, Lithuanian, Romanian and Czech: **15 more catalogs**
(24 interface languages including English).

## Upcoming

Ranked; detail and the full milestone table in [docs/roadmap.md](docs/roadmap.md).

| Rank | Milestone | Estimate (h) |
|---|---|---|
| 0 | Convert and share: Create PDF from Office, Word export that keeps layout (#773) | 32–55 |
| 1 | Backlog and platform: crash and wrong-result reports, Chinese web UI face (#826) | 50–90 |
| 2 | Fidelity harness against Acrobat and the renderer decision (M2; owner decision on #841) | 110–190 |
| 3 | Editing existing content (M7) | 100–180 |
| 4 | OCR languages and Office export/import (M10) | 95–175 |
| 5 | Preflight and PDF/A/X/UA (M11) | 65–115 |
| 6 | Signatures for institutions: PKCS #11, timestamp servers, online LTV (M9) | 40–70 |
| 7 | Accessibility tagging (M12) | 40–70 |
| 8 | 1.0 polish: performance, right-to-left, missing languages, keyboard-only (M14) | 55–95 |

## History

The progress log, revision history and per-session review banners live in
[docs/roadmap-history.md](docs/roadmap-history.md). This page keeps only the current summary.
