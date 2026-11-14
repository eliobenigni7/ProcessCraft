# Site redesign

Status: in progress (2026-11-14). Scope: the GitHub Pages site in `site/` and its workflow
`.github/workflows/pages.yml`. The app is unchanged.

## Decisions

- **Primary audience of the home page:** process analysts who draw, document and analyse
  processes. Developers and AI agents get their own pages (Automation, and later the Docs).
- **Visual identity:** the app's own. Colours come from `apps/processcraft/src/theme.rs` (amber
  accent, neutral greys, light and dark), the type is Inter and JetBrains Mono served from
  `assets/fonts` (no external resources), and radii and shadows follow the app tokens.
- **Main pages are hand-written HTML and image-led:** Home, Features, Automation, Download, Status.
  No scripts, no generator.
- **Docs are an mdBook under `/docs/`**, with three entry paths (analysts, developers, agents) over
  one set of pages. The CLI and MCP reference is generated from `processcraft-cli commands --json`
  in the Pages workflow so it cannot drift. Agents get `llms.txt`, `llms-full.txt` and a `.md`
  copy of each page. Internal plans and evidence stay in the repository, not in the docs.
- **Every image is real:** app screenshots from `docs/images/` (PrintWindow captures of the release
  build) or renderings made by `processcraft-cli export-image` from fixtures. Interactive app states
  cannot be captured headless (clicks do not reach a background winit window), so analyst views are
  shown through exports and through HTML built from real CLI output, and say so.
- **Claims stay measured:** numbers on the home page are taken from the Status page.

## Phases

1. Visual foundation: `style.css` from the app tokens, Inter `@font-face`, shared components
   (hero, image and text split, cards, number band, problems list).
2. Images: reuse `docs/images/`; add renderings from fixtures (procurement diagram, light and dark).
3. Home and Features rebuilt around images; Automation, Download and Status in the same style.
4. mdBook skeleton: theme aligned with the site, a "Who are you?" start page, Pages workflow step.
5. Docs content: analyst tutorial on the procurement example, generated CLI/MCP reference, agent
   pages and `llms.txt`.
6. Checks: internal links, phone width, dark mode, `scripts/check_text_hygiene.py`.
