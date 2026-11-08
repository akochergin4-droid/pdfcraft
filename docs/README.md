# PdfCraft documentation

Start here. Each row says what the page is for and whether it is current. The one-page project
summary is [ROADMAP.md](../ROADMAP.md); the day-to-day working rules are in [AGENTS.md](../AGENTS.md).

Status key: **living** is kept up to date as the code changes; **reference** is stable and only
changes when the thing it describes does; **historical** is a record of a past moment (do not read
it as current).

## Use the app

| Page | What it covers | Status |
|---|---|---|
| [word-count.md](word-count.md) | File ▸ Word count | living |
| [page-display.md](page-display.md) | Continuous page display and gaps | living |
| [localization.md](localization.md) | Choosing the interface language, adding a language | living |
| [editing-existing-text.md](editing-existing-text.md) | Editing existing text and source subsets | living |
| [editing-images.md](editing-images.md) | Editing existing images and figures | living |
| [editing-multiple-objects.md](editing-multiple-objects.md) | Moving several objects together | living |

## Automate it

| Page | What it covers | Status |
|---|---|---|
| [cli-batch.md](cli-batch.md) | `pdfcraft-cli batch`: conversion, OCR and compression | living |
| [mcp.md](mcp.md) | MCP server conventions (opt-in, stdin/stdout) | living |

## Build, ship and contribute

| Page | What it covers | Status |
|---|---|---|
| [development.md](development.md) | Environment variables, diagnostics, build profiles (including faster builds) | living |
| [architecture.md](architecture.md) | How PdfCraft is built, crate by crate | living |
| [releasing.md](releasing.md) | The `release` branch, installers, signing | reference |
| [licensing-self-check.md](licensing-self-check.md) | `cargo xtask license-check` | reference |
| [contributors.md](contributors.md) | How the About ▸ Contributors window is fed | reference |
| [windows-renderer-startup.md](windows-renderer-startup.md) | Windows startup errors and the OpenGL fallback | reference |
| [form-performance.md](form-performance.md) | AcroForm JavaScript performance design and measurements | reference |

## Where the project stands

| Page | What it covers | Status |
|---|---|---|
| [target-app-parity.md](target-app-parity.md) | The authoritative assessment against Acrobat Pro | living |
| [gaps.md](gaps.md) | Every known shortfall, ranked: the work list | living |
| [roadmap.md](roadmap.md) | Milestones M0–M14 and what is next | living |
| [pdf-spec-parity.md](pdf-spec-parity.md) | ISO 32000-2 and PDF/A/X/UA coverage | living |
| [file-format-parity.md](file-format-parity.md) | Formats read and written | living |
| [ui-parity.md](ui-parity.md) | Tools, panels, shortcuts | living |
| [hardware-parity.md](hardware-parity.md) | GPU, printers, scanners, platforms | living |
| [localization-parity.md](localization-parity.md) | Per-language status | living |
| [roadmap-history.md](roadmap-history.md) | Progress log and revision history moved out of ROADMAP.md | historical (append-only log) |

## Research and archive

| Page | What it covers | Status |
|---|---|---|
| [research/pdf-workflows/](research/pdf-workflows/README.md) | Archived comparative research and source register | reference |
| [archive/egui-037-migration-plan.md](archive/egui-037-migration-plan.md) | Working notes from the egui 0.37 migration | historical |
| [archive/integration-20261030.md](archive/integration-20261030.md) | One-off integration candidate of 2026-10-30 | historical |

`brand/` holds the ArtCraft logos (not open source; see `brand/LICENSE-brand.txt`) and `images/` the
screenshots used by the README.
