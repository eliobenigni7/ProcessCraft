# UX/UI shell redesign — evidence

Plan: [`docs/plans/ux-shell.md`](../plans/ux-shell.md). Every step below passed `cargo fmt --all -- --check`,
workspace all-target/all-feature Clippy with `-D warnings`, and the workspace all-target/all-feature
tests. Native screenshots of the release build (1560×960, sample project from
`tests/fixtures/commands/procurement.json`) are under `target/ux-shell/` (not committed).

## 1. Foundation

- **Tokens:** `apps/processcraft/src/theme.rs` holds the light-first tokens. The amber app colour is
  the accent, with `#9a6400` as the ink colour for amber text; blue is kept for canvas selection.
- **Shape and density:** radii 4/6/8/12, 28 px controls, and a 13 px body text with an 11–16 px type
  scale.
- **Widget states:** set through `craft_ui::theme::WidgetPalette`. Controls show their state by fill;
  there are no resting outlines.
- **Interface fonts:**
  - Inter Regular, Medium and SemiBold, plus JetBrains Mono, all installed through
    `craft_ui::fonts::UiFonts`.
  - The bundled Noto Arabic and CJK faces act as fallbacks, and system fonts are off.
  - The four new faces are byte-identical to WordCraft's pinned set. They are recorded in
    `assets/fonts/provenance.json` (OFL-1.1, SHA-256 checked by `text_parity`).
  - They are used only by the interface; diagram rendering and export still use Noto.
- **Test:** `theme::tests::interface_fonts_cover_latin_arabic_and_chinese` checks Latin, monospace,
  Arabic and Chinese coverage.
- **Screenshot:** `step1-foundation-light.png`.
