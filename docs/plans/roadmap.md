# Roadmap after M4

ProcessCraft today models, validates, documents and exchanges BPMN offline on one machine. This roadmap
takes it toward the analyst and consultant work that SAP Signavio Process Manager covers: process
attributes, conventions, publishing, process architecture, simulation and decisions. It stays offline,
single-user and agent-ready. Process mining (the Celonis side) comes last and is optional.

Each milestone gets its own detailed plan in `docs/plans/` and an evidence file in
`docs/implementation/` before it is called done, as M0–M4 did. The architectural invariants in
`docs/design/solution-design.md` hold throughout: one command pipeline for GUI, CLI and MCP; `io` and
`render` never depend on each other; content outside a qualified profile is preserved or refused.

## Positioning

| Keep out of scope | Why |
|---|---|
| Real-time collaboration, cloud repository, accounts, authenticated approvals | Contradicts the offline, local product; would be a different product |
| ERP connectors and a data platform | Celonis' moat; nothing to win there |
| Process execution engine | Simulation (M7) estimates behaviour; it never runs processes or calls systems |
| A built-in generative AI provider | Agents reach ProcessCraft through MCP; ProcessCraft ships prompts, not a model client |

## Overview

| Milestone | Theme | Main deliverables | Size | Depends on |
|---|---|---|---|---|
| **M4** (finish) | Ship | Signing, accessibility and performance qualification | M | — |
| **M5** | Analyst foundations | Custom attributes, modeling conventions, RACI, heatmap overlay, MCP prompt pack | L | M4 |
| **M6** | Publish and structure | Process handbook (static HTML + PDF), process map with drill-down, semantic diff | L | M5 |
| **M7** | Analyse | Discrete-event simulation with what-if comparison; Visio import | XL | M5 |
| **M8** | Decide | DMN decision tables linked to business rule tasks | L | M5 |
| **M9** (optional) | Observe | Local event-log conformance and overlay; discovery later | XL | M5, M7 |

M6, M7 and M8 are independent of each other once M5 lands and can be reordered. M8 and M9 need an
amendment of the non-goals in `solution-design.md` (DMN editor, process mining) before their plans
start.

## M4 — finish shipping

Remaining from the M4 plan (`m0-m1-implementation-plan.md`, rows M4.1–M4.2):

- **Signing**: SignPath Foundation open-source signing wired into `release-windows.yml`; signed MSI and
  ZIP verified with `signtool verify`.
- **Installer**: per-machine install, full wizard, upgrade from a lower version, rollback and the
  first CI release run are recorded in `m4-evidence.md` (2026-11-13); what remains is listed there
  under "Not established".
- **M4.2 accessibility and performance**: keyboard-only modeling, table and dictionary workflows;
  Narrator; focus, contrast, DPI; recorded timing and memory on a declared machine.

Acceptance: a tagged release produces signed packages; the M4.2 checks are recorded with measured
limits.

## M5 — analyst foundations

The base every later milestone builds on.

**Status:** released in 0.2.0 (2026-11-13), not yet closed. M5.1 to M5.5 are implemented, the prompt pack has two rounds of independent agent runs, and M5.6 has recorded its automatable qualification (gates, CLI workflow, measurements; `docs/implementation/m5-evidence.md`). Open before M5 can close: the GUI and attached-MCP procurement workflows and one external BPMN tool check, all by hand.

### M5.1 Custom attributes

- Project-level attribute definitions: name, type (text, number, duration, currency, enum, date,
  dictionary reference, URL), applicable element kinds, required flag.
- Values edited in the inspector and as columns of the table view; batch-edited through the existing
  `Mutation` pipeline, so CLI and MCP get them for free.
- Native format: definitions and values in the project JSON behind a new required capability (as
  comments and checkpoints are), not a schema bump.
- BPMN export: values in a ProcessCraft `extensionElements` namespace; import reads them back and
  preserves foreign extensions as today.
- Starter templates: Duration, Cost, SLA, Risk, Control, Process owner. Node owner/contact and IT
  systems (dictionary `System` links) already exist and stay as they are.

Delivered in three phases (M5 plan): M5.1a model, commands, native format, CLI and MCP; M5.1b BPMN
interchange; M5.1c desktop editing.

Acceptance: round trip native → BPMN → native keeps every value; a foreign tool's extensions survive
alongside; deleting a definition, removing an enum option or narrowing the element kinds while values
would be orphaned asks for an explicit resolution, as deleting a used dictionary entry does. A
definition's type never changes.

### M5.2 Modeling conventions

- A conventions profile stored in the project: rule ID, severity, parameters.
- First rules: label required; activity label as verb + object (configurable word lists, English
  first); labelled gateway outgoing flows; no implicit split or join; one start and one end per
  process level; maximum elements per diagram; required attributes (from M5.1); dictionary terms used
  instead of free text.
- Results in the existing problems panel with navigation; `validate --profile` in the CLI with JSON
  output and a non-zero exit for CI.

Acceptance: every rule has a passing and a failing fixture; results are identical across GUI, CLI and
MCP (today CLI and MCP `validate` run only the structural check, so they first gain the diagnostics
the GUI already shows).

### M5.3 RACI matrix

- A view derived from lanes and dictionary roles (Responsible), with manual A/C/I assignments stored
  as attributes.
- Export to CSV and to the handbook (M6.1).

### M5.4 Attribute overlay

- Colour activities by a numeric or enum attribute (heatmap) or show a value badge; legend; screen
  only by default, optional in image and PDF export.
- The same overlay mechanism later shows simulation (M7) and conformance (M9) results.

### M5.5 MCP prompt pack

- Documented prompts and skills: text/SOP → model, review against the conventions profile, explain a
  process for business readers, fill attributes from a description.
- No code in ProcessCraft beyond what MCP already exposes; checked by a scripted run against a
  fixture.

## M6 — publish and structure

### M6.1 Process handbook

- Export one project to a static, self-contained HTML site: process map, clickable diagrams (SVG from
  the shared scene), one page per activity with description, attributes, RACI and comments, dictionary
  and glossary, search without a server.
- The same content as a PDF, printed by the browser through a print stylesheet. The existing PDF
  export outlines text and draws one scene; a native text layer and page layout come only if the
  print route falls short (see the M6 plan).
- No scripts from remote origins; the site opens from `file://` and a file share.

Acceptance: the generated site passes an HTML validator and a link check and works from `file://`; if
a native PDF is built, its text is extractable.

### M6.2 Process map and drill-down

- A process landscape document kind: value chain and hierarchy levels that link to processes in the
  project.
- Call activities and collapsed sub-processes resolve across documents of the project; a drill-down
  gesture (double-click already edits labels) opens the target, breadcrumb returns.
- Links stay valid: deleting a linked document warns first instead of leaving a dangling link.

### M6.3 Semantic diff

- `processcraft-cli diff <a> <b>` (native or BPMN) reports added, removed and changed elements,
  attributes and flows; JSON output; usable as a git `difftool`.
- A visual overlay comparing two checkpoints in the GUI, building on the existing checkpoint
  comparison.

## M7 — analyse

### M7.1 Simulation

- Scenarios as project objects (seeded from attributes, so what-if can hold two): arrival rate and calendar, activity duration distributions,
  gateway branch probabilities, resources per lane or role with cost and capacity.
- Engine in `processcraft-core` (or a new core-only crate if the size justifies it): deterministic
  seeded discrete-event simulation, bounded run length and instance count.
- Outputs: cycle time distribution, waiting versus working time, resource utilisation, cost per
  instance, bottleneck ranking; shown through the M5.4 overlay and a results panel.
- What-if: compare two scenarios or two checkpoints side by side.
- CLI: `processcraft-cli simulate --scenario … --json`.

Acceptance: analytical checks on simple models (an M/M/1 queue, serial and parallel paths) match
within stated tolerances; same seed gives the same result on GUI and CLI.

### M7.2 Visio import

- Import `.vsdx` diagrams drawn with the Visio BPMN stencils into an editable model, with a report of
  everything approximated or dropped.
- Fixtures with provenance and licence like the existing ones.

## M8 — decide (requires a non-goal amendment)

- DMN 1.6 as a separate document kind, as `solution-design.md` already anticipates: decision tables
  with hit policies, input and output columns, typed values.
- A business rule task links to a decision; the link resolves like call activities in M6.2.
- Evaluation of decision tables with a bounded subset of FEEL (S-FEEL first); a test panel with
  example inputs.
- DMN XML import and export with the same preservation rule as BPMN.

## M9 — observe (optional, requires a non-goal amendment)

Only after M5–M7 have users. Delivered in slices:

1. Event-log import: CSV with column mapping, XES; OCEL 2.0 later. Bounded size, streaming parser.
2. Conformance: token replay against a BPMN model, deviations listed and shown on the diagram.
3. Performance overlay: frequencies and durations from the log through the M5.4 overlay; feeds M7
   scenario inputs from measured data.
4. Discovery: directly-follows graph, then an inductive miner producing an editable BPMN model;
   variant explorer.

Everything stays on the machine; no connectors.

## Decisions taken (2026-11-13)

The recommended defaults in the milestone plans' "Open decisions" are adopted, except the M6 PDF,
where the plan's simpler alternative was taken. The plans now list them under "Decisions":

- M5: attributes ship behind a required native capability, not a schema bump; starter set Duration,
  Cost, SLA, Risk, Control, Process owner, leaving node owner/contact and dictionary `System` links
  as they are; extension namespace `urn:processcraft:attributes:1`; `validate` always applies the
  stored profile, `--profile <FILE>` overrides it, exit 3 on errors only.
- M6: the handbook is one self-contained `.html`; PDF comes from a print stylesheet first, a native
  PDF text layer only if that falls short; double-click keeps editing labels and drill-down gets its
  own gesture; navigation links stay non-dangling, with a warning before deleting a linked document.
- M7: scenarios are project objects seeded from attributes.
- M8, M9: the proposed amendment texts are accepted; `solution-design.md` is amended when each
  milestone starts. The DMN version is confirmed against OMG at M8 start.
- M9: raw logs stay outside the project, which stores only mappings and derived results; discovery
  (slice 4) is decided after slices 1–3.

Still open: who supplies real Visio fixtures for M7.2 (it does not start without them).
