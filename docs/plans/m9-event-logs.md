# ProcessCraft M9 Plan — Local event logs (optional)

**Goal:** Deliver M9 of the [roadmap](roadmap.md) in four slices: import an event log from a local
file, check it against a BPMN model, show frequencies and durations on the diagram, and — last —
discover a model from a log. Everything stays on the machine.

**Status:** Optional and secondary. Planned at slice level only; a task-level plan is written when
the milestone is scheduled, which the roadmap ties to M5–M7 having users. The amendment text below
was accepted on 2026-11-13 and is written into `solution-design.md` when M9 starts.

## Scope amendment required

`docs/design/solution-design.md` line 67 (§1.2 Initial exclusions) currently reads:

> No process execution engine, process mining, distributed repository, authenticated approvals,
> real-time collaboration, scripting language, plugin marketplace, cloud synchronization, generative
> AI provider, or full DMN editor.

M9 is process mining in the plain sense of the term, so the exclusion has to be narrowed, not
reinterpreted.

**Amendment text** (accepted; written into `solution-design.md` when M9 starts):

1. In the sentence at line 67, replace "process mining" with "process mining beyond the local
   event-log scope of §1.2.2".
2. Add after the §1.2 paragraphs:

   > #### 1.2.2 Amendment (M9): local event-log analysis
   >
   > In scope: importing one event-log file that the user selects (CSV with an explicit column
   > mapping, or XES), replaying it against a BPMN model of the project, listing deviations, showing
   > frequencies and durations on the diagram, and deriving a draft model from the log.
   >
   > Still excluded: connectors to source systems, scheduled or streaming ingestion, a data platform
   > or data model beyond one imported log, object-centric logs until separately specified, root-cause
   > or predictive analytics, and any transfer of log data off the machine. Raw event data is not
   > stored inside the `.processcraft` package unless the user explicitly asks for it.

3. In §17.2 add the event-log limits chosen in slice 1 to the limits table.

## Boundaries and architecture

- Parsing is in `processcraft-io` (new `eventlog.rs`): streaming, bounded, no new dependency assumed
  (XES through the pinned `quick-xml`; CSV through a small RFC 4180 reader). Analysis is pure code in
  `processcraft-core` (new `eventlog.rs` for the in-memory log, `conformance.rs`, later
  `discovery.rs`). `io` fills a core type; core never reads files.
- The in-memory log is variant-compressed: interned activity names, distinct traces with counts, and
  per-activity duration aggregates. Raw rows are not retained after import.
- Replay runs on the flattened process graph that M7 builds (`simulation::prepare`), so M9 supports
  and refuses exactly the constructs M7 does. That is the reason for the M7 dependency.
- Results reach the diagram only through the M5.4 `Overlay`.
- Log analysis results are transient, like simulation results. What is saved in the project is
  small and explicit: the column mapping and the activity-to-element mapping.

## Slices

### Slice 1 — Event-log import

- `eventlog::read_csv(grant, &CsvMapping)` and `read_xes(grant)` returning the core `EventLog`.
  Required columns: case ID, activity, timestamp; optional: end timestamp or lifecycle, resource.
  XES subset: `concept:name`, `time:timestamp`, `lifecycle:transition`, `org:resource`; other
  attributes are ignored and counted in the report.
- Timestamps: ISO 8601 and epoch seconds or milliseconds first; a declared pattern from a short
  fixed list for CSV. No locale guessing.
- Limits (proposed, to be measured): 512 MiB file, 5,000,000 events, 1,000,000 cases, 10,000
  distinct activities, 64 KiB per line. Exceeding a limit stops with a named limit and no partial
  log.
- Column mapping UI with a preview of the first rows; CLI `log-info <FILE> --mapping <FILE.json>`.
- **Acceptance:** fixtures (synthetic, plus one public log with a recorded licence) import with the
  expected case, event and variant counts; malformed rows are counted and reported by line number,
  not skipped silently; oversized inputs are refused.

### Slice 2 — Conformance by token replay

- Activity mapping: exact name match proposed, every mapping confirmed or edited by the user, saved
  through one mutation (`SetLogMapping`). Unmapped log activities and unmapped model activities are
  listed.
- `conformance::replay(graph, log, mapping, cancel)` reports per variant: fit or not, missing and
  remaining tokens, and the element where each deviation occurs; plus log-level fitness by the
  standard token-replay formula. Bounded by variants × trace length with a cancellation check.
- Deviations list in a bottom tab with navigation to the element; deviation counts as an `Overlay`.
- CLI `conformance <PROJECT> --log <FILE> --mapping … --json`; MCP query of the same result.
- **Acceptance:** hand-built logs with known deviations (skipped activity, wrong order, extra
  activity, loop) produce exactly the expected missing/remaining counts and fitness; a perfectly
  fitting log gives fitness 1; refused constructs give diagnostics and no result.

### Slice 3 — Performance overlay and simulation inputs

- Per activity: frequency, mean and percentile duration (when the log has start and end, otherwise
  time since the previous event, labelled as such); per sequence flow: frequency. Shown through the
  M5.4 overlay with a legend naming the log and its date range.
- "Create scenario from log": builds an M7 `Scenario` with the measured interarrival time, branch
  probabilities from flow frequencies and activity durations, as one `CreateScenario` mutation the
  user can undo.
- **Acceptance:** for a synthetic log generated from known parameters, the derived scenario's
  parameters match within stated tolerances, and simulating it reproduces the log's mean cycle time
  within a stated tolerance.

### Slice 4 — Discovery (later)

- Directly-follows graph and a variant explorer as tables first (variant, share, mean duration);
  no new graph notation is drawn.
- An inductive miner (base algorithm, then the infrequent-behaviour variant) producing a process
  tree, converted to a BPMN document with tasks and exclusive and parallel gateways, laid out by the
  existing `layout::propose_layout` within its limits (400 shapes, 500 edges), and committed as one
  `CreateDocument` batch. The result is marked as a draft derived from a named log.
- **Acceptance:** logs generated from known block-structured models are rediscovered up to
  behavioural equivalence on the fixtures; replaying the source log on the discovered model gives
  fitness 1 for the base algorithm; models beyond the layout limits are refused, not truncated.

## Key decisions

| Decision | Adopted | Why |
|---|---|---|
| Where raw log data lives | Outside the project; the project keeps mappings only, results are recomputed from a file the user re-selects | Logs are large and often contain personal data; `.processcraft` files are shared and checkpointed |
| CSV parser | Small in-crate reader | Avoids a new pinned dependency for a bounded, well-specified format |
| Conformance method | Token replay only | Alignments are far more expensive and need a different engine; revisit on demand |
| Supported BPMN subset | Same as M7 | One flattening function, one refusal list |
| Discovery algorithm | Inductive miner | Always yields a sound, block-structured model that maps directly to BPMN |
| OCEL 2.0 | Not in M9 | Roadmap says "later"; it changes the log model |

## Risks

- **Personal data.** Resource names and case identifiers are personal data in many logs. Mitigation:
  raw data is not persisted, overlays and reports are aggregates, and the evidence file says so.
- **Memory on large logs.** Mitigation: streaming parse into the variant-compressed form; limits
  measured in slice 1 before they are published.
- **Misleading precision.** Durations derived from single-timestamp logs are not service times.
  Mitigation: label the measure in every view and export.
- **Name-based mapping errors.** Mitigation: mappings are proposed, never applied without
  confirmation; unmapped items are always listed.
- **Scope pull toward a mining product.** Mitigation: the amendment's exclusion list; no connectors,
  no scheduling, no data model.

## Evidence

One `docs/implementation/m9-evidence.md`, updated per slice, with fixture tables (provenance,
licence, hash), measured limits and the acceptance results above. No comparison with mining products
is claimed.

## Decisions (adopted 2026-11-13)

The recommended option of each former open decision was adopted (roadmap, "Decisions taken")
unless stated otherwise below. The alternatives are kept for the record.

1. **Amendment text above:** accepted.
2. **Persisting log data.** Adopted: mappings only. Alternative: optionally embed the
   variant-compressed log as a project `Resource` (bounded by the 32 MiB resource limit) so results
   can be reproduced from the project file alone — with the privacy cost stated in the UI.
3. **Whether slice 4 is wanted at all.** Adopted: decide after slices 1–3 have been used;
   discovery is the largest part and the furthest from the analyst workbench.
4. **Public fixture log.** Adopted: one small public log with a clear licence for realism, plus
   synthetic logs for exact expectations; the user confirms the licence before it is committed.
5. **Limits.** The figures in slice 1 are proposals; adopted: fix them from measurements on the
   qualification machine.
