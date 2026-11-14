# Changelog

Notable changes to ProcessCraft. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While the project is
pre-alpha (0.x), the `.processcraft` format and the command protocol may change between minor
versions; such changes are listed here.

## [Unreleased]

## [0.2.2] - 2026-11-14

Unsigned Windows build (x64): MSI installer and portable ZIP.

### Added

- **Move an edge's attach point:** drag an end handle of a selected flow near its shape to dock it
  on any side, on the outline of rectangles, events and gateways, with snapping to the side's middle,
  the next bendpoint or a straight line to the other end. The route is redrawn orthogonally and the
  change is one undo step; the end keeps its place on that side when the shape later moves.

### Changed

- **Auto-layout:** fewer crossings and no shared channels in lane-heavy processes. Legs in the same
  gap no longer cross, several flows on one side of a shape spread along its outline, and a long
  flow between lanes can run in its target lane. A 48-node, five-lane litigation process goes from
  6 crossings to the 1 its lane order forces.

### Fixed

- Releasing a dragged edge end on or just outside its own shape now re-docks it (before, only a
  release strictly inside the shape did).

## [0.2.1] - 2026-11-14

Unsigned Windows build (x64): MSI installer and portable ZIP.

### Added

- **Resize every shape:** tasks, call activities, collapsed subprocesses, events, gateways, data
  objects and stores can be resized; events, gateways and data objects and stores keep their
  proportions. Eight handles with resize cursors, snapping to neighbours and the grid, Shift to keep
  proportions, Alt to resize around the centre, Ctrl to resize without snapping. A resized shape
  grows its lane, pool or subprocess.
- **Fit to content** (Edit menu, or double-click a resize handle) fits an activity's or annotation's
  height to its text.
- Hover outline, Shift to keep a move on one axis, a right-to-left selection box that takes the
  shapes it touches, Shift+1 to fit the diagram, Shift+2 to zoom to the selection, F2 to edit a
  label.
- Auto-layout previews report the crossings and conflicts that remain as `warnings` and `metrics`
  (CLI, MCP and the preview bar) instead of refusing the layout.
- Documentation book at <https://eliobenigni7.github.io/ProcessCraft/docs/> with paths for analysts,
  developers and AI agents, a procurement tutorial, a CLI and batch-command reference generated from
  `processcraft-cli`, and `llms.txt` / `llms-full.txt` for agents.
- Contributor guide, security policy, code of conduct, support page, and issue and pull request
  templates.

### Changed

- **Connections:** rerouted edges are cleaned up, explicit labels follow their edge, ends dock on the
  outline of events and gateways and move to the side facing their neighbour when a shape moves or
  is resized, and manual bendpoints are kept. Dragging a bendpoint or segment keeps the route
  orthogonal and the ends on the shape border; reconnecting keeps bendpoints; the connect preview
  shows the real route.
- Associations and data associations are drawn straight. Edges are drawn as one path with small
  rounded corners and continuous dashes, on the canvas and in SVG, PNG and PDF exports.
- **Auto-layout:** a layered layout with crossing reduction, rows and columns sized to their
  content, lanes indented inside the pool and flows routed on separate tracks. It now accepts every
  flow node; boundary events, expanded subprocesses, artifacts, several pools and message flows are
  still refused.
- The website follows the app's visual identity and is built around images of the app and its
  exports.

### Known limits

- The new canvas gestures have not been checked by hand in the running app. Measured limits are in
  `docs/implementation/m3-evidence.md`.

## [0.2.0] - 2026-11-13

Unsigned Windows build (x64): MSI installer and portable ZIP.

### Added

- **Custom attributes:** typed definitions (text, number, duration, currency, enum, date, dictionary
  reference, URL) and per-element values, edited in the inspector, the activity table (multi-row
  edits) and the Dictionary view, and available through the CLI and MCP.
- **Attributes in BPMN:** values travel in a `pc:attributes` extension element
  (`urn:processcraft:attributes:1`); malformed or unplaceable values are kept as extension content,
  never dropped.
- **Modeling conventions:** a project profile of eight configurable rules, shown with the built-in
  diagnostics; `processcraft-cli validate --profile FILE --fail-on warning|error` for CI.
- **RACI matrix:** R from lane roles, A, C and I from attributes; a desktop grid, `query raci` and
  CSV export.
- **Attribute overlay:** heatmap or badges on the canvas, optional in SVG, PNG and PDF exports with a
  legend.
- **MCP prompt pack** (`docs/prompts`): four agent instructions, checked against the live schemas and
  replayed from recorded runs; new MCP and CLI queries `documents` and `conventions`.

### Changed

- Projects that use attributes or conventions need 0.2.0 or later to open (native capabilities
  `analyst-attributes-v1` and `modeling-conventions-v1`). Existing projects keep their content
  identity.

### Known limits

- The desktop workflow, the attached MCP workflow and a round trip through an external BPMN tool have
  not been checked by hand. Measured limits are in `docs/implementation/m5-evidence.md`.
- No BPMN 2.0.2 conformance claim is made.

## [0.1.0-rc.1] - 2026-11-13

First public prerelease: an unsigned Windows build (MSI installer and portable ZIP).

### Added

- Desktop BPMN modeling with bpmn-js-style interaction: context pad, rule-checked connections,
  automatic placement and routing, replace, lanes, pools, sub-processes, boundary events, space tool,
  clipboard, align and distribute, auto-layout with preview, undo and redo.
- Analyst workbench: model library with folders, dictionary of roles, systems and documents, activity
  table, comments and checkpoints.
- BPMN 2.0 XML import and export with diagram interchange, in three import modes (editable, editable
  with retained extensions, preserved source); SVG, PNG, PDF and draw.io export.
- The native `.processcraft` package (schema 3) with atomic save, a single-writer lock and recovery.
- `processcraft-cli` for batches, queries, validation, import, export and layout, and a local MCP
  server, standalone or attached to the open desktop session.
- Per-user Windows installer with file association.

[Unreleased]: https://github.com/eliobenigni7/ProcessCraft/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/eliobenigni7/ProcessCraft/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/eliobenigni7/ProcessCraft/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/eliobenigni7/ProcessCraft/compare/v0.1.0-rc.1...v0.2.0
[0.1.0-rc.1]: https://github.com/eliobenigni7/ProcessCraft/releases/tag/v0.1.0-rc.1
