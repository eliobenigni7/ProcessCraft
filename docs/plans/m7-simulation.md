# ProcessCraft M7 Implementation Plan — Simulation and Visio import

**Goal:** Deliver M7.1 and M7.2 of the [roadmap](roadmap.md): a deterministic discrete-event
simulation of a BPMN process with what-if comparison, and an import of Visio `.vsdx` diagrams drawn
with the BPMN stencil into an editable model with a report of what was approximated or dropped.

**Architecture:** The simulation engine is a pure module in `processcraft-core` (standard library
only, no clock, no threads, no file access). Scenarios are project content changed through
`Mutation`s; results are transient and recomputed. The Visio reader is a bounded parser in
`processcraft-io` that reuses the pinned `zip` and `quick-xml` dependencies. The two tasks share no
code and can be delivered in either order.

**Authority:** [roadmap](roadmap.md) §M7; [solution design](../design/solution-design.md) §1 (the
workbench documents work, it does not execute it), §5.3 (explicit units, decimal strings), §6
(preserve or refuse), §15 (jobs off the UI thread), §17 (bounded inputs). Depends on
[M5](m5-analyst-foundations.md): attributes (M5.1) and the overlay (M5.4). Uses M6.2 `drill_target`
when present.

**Status:** Planned; not started.

## Boundaries

In scope: estimating time, waiting, utilisation and cost for one process under stated assumptions;
converting a Visio BPMN drawing into a ProcessCraft document.

Out of scope, and not to be scaffolded:

- Execution. The simulator never evaluates condition expressions, scripts or service bindings, never
  calls a system and never reads process data. Branching is by stated probability only. This keeps
  M7 inside the existing non-goal "no process execution engine" without an amendment; see Open
  decision 7.
- BPSim interchange, optimisation or parameter search, animation of tokens on the canvas.
- Simulating several pools with message exchange; one process is simulated at a time.
- Visio drawings that do not use the BPMN stencil, Visio `.vsd` (binary) files, and export to Visio.

## Findings from the current code that shape this plan

1. **"Scenario inputs stored as attributes" does not fit what-if comparison.** An M5.1 attribute
   holds one value per element, so two scenarios of the same model cannot both live in attributes;
   arrival rate, calendar and resource pools are not properties of a model element at all; and the
   M5.1 type list has no distribution type. This plan stores scenarios as project content and reads
   M5.1 attributes as their defaults. See Open decision 1.
2. **No random number or statistics crate is a direct dependency of `processcraft-core`** (`rand`
   appears in `Cargo.lock` only transitively). The engine carries its own small generator, which
   also pins results across dependency updates.
3. **`layout.rs` is the precedent for bounded, cancellable, pure computation:** `propose_layout`
   takes `cancel: &AtomicBool`, refuses unsupported structures with a `LayoutError` and returns no
   partial result. The simulator follows the same contract.
4. **The native ZIP reader's entry checks are private to `native.rs`** (`load_with_limits`: path
   safety, compression method, duplicate names, per-entry and expanded size caps). The Visio reader
   needs the same checks; they are extracted into one shared function rather than copied.
5. **`processcraft-cli` already hosts qualification harnesses as examples**
   (`apps/processcraft-cli/examples/*_qualify.rs`); the analytical checks use that form.

## Task M7.1: Simulation

**Own:** new `crates/processcraft-core/src/simulation.rs` (split into a `simulation/` folder only if
it passes roughly 1,500 lines); `crates/processcraft-core/src/{model,commands,identity,history,
overlay,lib}.rs`; `crates/processcraft-io/src/native.rs`; `apps/processcraft/src/{jobs.rs,
app/shell.rs,app/inspector.rs}`, new `apps/processcraft/src/app/simulation.rs`;
`apps/processcraft-cli/src/{args,run,mcp}.rs`; `apps/processcraft/src/attachment.rs`; new
`apps/processcraft-cli/examples/simulation_qualify.rs`.

### Supported model subset

- [ ] `simulation::prepare(project, document_id, process_id) -> Result<ProcessGraph, Vec<Diagnostic>>`
  flattens one process of an `Capability::Editable` document into a graph of steps. Supported: none
  start and end events; tasks of every kind (as timed work); exclusive and event-based gateways
  (probabilistic choice); parallel gateways (split and synchronising join); intermediate timer events
  (delay); embedded sub-processes (inlined); call activities (inlined when `drill_target` resolves
  inside the project, otherwise a timed task); loops through gateways.
- [ ] Everything else is refused with one diagnostic per element and no partial run: inclusive and
  complex gateways, boundary events, event sub-processes, multi-instance and loop characteristics,
  compensation, transactions, multiple start events, processes without a start or an end event.
  The list of supported and refused constructs is documented in the CLI help and the evidence file.

### Scenario

- [ ] Model in `simulation.rs`, `deny_unknown_fields`, decimal strings for money, integer
  milliseconds for time:
  - `Scenario { id, name, document_id, process_id, arrivals: Arrivals, calendar: Option<WeekCalendar>,
    resources: Vec<ResourcePool>, activities: Vec<ActivityParams>, branches: Vec<BranchParams>,
    limits: RunLimits, seed: u64 }`.
  - `Arrivals { interarrival: Distribution, max_instances: u32 }`.
  - `Distribution`: `Fixed`, `Uniform { min, max }`, `Exponential { mean }`, `Triangular { min, mode,
    max }`, `Normal { mean, sd }` (truncated at zero); parameters are ISO 8601 duration strings as
    in M5.1.
  - `ResourcePool { id, name, role: Option<DictionaryEntryId>, lane: Option<ElementKey>, capacity:
    u32, cost_per_hour: Option<String> }`. An activity uses the pool of its lane or of its lane's
    role; an activity without a pool has unlimited capacity.
  - `ActivityParams { element: ElementSelector, duration: Option<Distribution>, fixed_cost:
    Option<String> }` and `BranchParams { flow: ElementSelector, probability: String }`.
  - `WeekCalendar`: up to 21 working intervals per week, applied to arrivals and to all pools.
    Per-pool calendars are not in M7.
  - `RunLimits { max_events, max_simulated_ms }` with hard ceilings of 100,000 instances and
    5,000,000 events.
- [ ] Defaults from M5.1: when `ActivityParams.duration` is absent, the element's `Duration`
  attribute is used as a fixed duration; when `fixed_cost` is absent, its `Cost` attribute. An
  activity with neither is reported and treated as zero duration only after explicit acknowledgement
  in the run request. Outgoing branches of a gateway without probabilities are refused, not assumed
  equal.
- [ ] Storage and commands: `ProjectData.scenarios: Vec<Scenario>` (serde default, skipped when
  empty), included in `FrozenProject`; native capability `"simulation-scenarios-v1"`; mutations
  `CreateScenario`, `SetScenario` (whole-value replace) and `DeleteScenario` with descriptors;
  validation of references, probability sums per gateway (each in 0..=1, summing to exactly 1
  as decimals) and limits in `ProjectData::validate()`. Element deletion removes the
  matching `ActivityParams`/`BranchParams` through the M5.1 pruning helper.

### Engine

- [ ] `simulation::run(graph: &ProcessGraph, scenario: &Scenario, cancel: &AtomicBool) ->
  Result<SimulationResult, SimulationError>`. Event queue ordered by (time in integer milliseconds,
  sequence number), so ties are deterministic. Sampled durations are rounded to milliseconds before
  use. First-in-first-out queue per pool. The cancel flag and the event budget are checked every
  fixed number of events; exhaustion returns an error and no result.
- [ ] Generator: SplitMix64 seeding xoshiro256** in about twenty lines, with published test vectors
  in a unit test. Samples are drawn in a fixed order per event so results do not depend on map
  iteration; every collection that affects order is a `Vec` or `BTreeMap`.
- [ ] `SimulationResult { completed, unfinished, cycle_time: Summary, activities:
  Vec<ActivityResult { element, count, working_ms, waiting_ms, cost }>, pools: Vec<PoolResult {
  pool, busy_ms, available_ms, utilisation, cost }>, cost_per_instance: Summary, bottlenecks:
  Vec<ElementSelector>, warnings }`. `Summary` holds mean, minimum, maximum, p50, p90, p95 and a
  20-bucket histogram. Bottlenecks are activities ranked by total waiting time. The result records
  the content identity, scenario and seed it was computed from.
- [ ] `overlay::simulation_overlay(&SimulationResult, metric)` builds an M5.4 `Overlay` for waiting
  time, working time, frequency or cost per activity.
- [ ] `simulation::compare(a: &SimulationResult, b: &SimulationResult) -> Comparison`: per metric
  and per activity (matched by `ElementSelector`), A, B and the difference; elements present on one
  side only are listed. Used for two scenarios of one model and for one scenario run against two
  checkpoints (`Checkpoint.content.project()`).

### Callers

- [ ] CLI: `simulate <INPUT> --scenario <ID> [--seed <N>] [--checkpoint <ID>] [--compare-scenario
  <ID> | --compare-checkpoint <ID>] [--acknowledge-missing-inputs] --json`, exit 3 when the model is
  refused. MCP: `Query::Simulate` in `mcp.rs` and `attachment.rs`, read-only, following the
  `LayoutPreview` precedent.
- [ ] Desktop: `simulation.rs` adds a scenario list and editor (arrivals, calendar, pools); the
  inspector gets a "Simulation" section for the selected activity or gateway flow while a scenario
  is open, with "Use attribute value" shown where the default applies. Run executes in a
  latest-wins worker in `jobs.rs` with Cancel; a result computed for another content identity is
  discarded. Results appear in a new bottom tab (`BottomTab` in `shell.rs`) with summary, activity
  table, pool table and histogram, a metric selector that drives the overlay, and a two-column
  comparison when a second run is selected.

### Tests and qualification

- New `crates/processcraft-core/tests/simulation.rs`:
  - generator vectors; each distribution's sample mean and variance within tolerance at 100,000
    samples with a fixed seed;
  - serial path of fixed durations: cycle time equals the sum exactly;
  - parallel split and join of fixed durations: cycle time equals the maximum exactly;
  - exclusive gateway 0.3/0.7: branch frequencies within one percentage point at 100,000 instances;
  - M/M/1 (exponential interarrival and service, one resource): mean time in system within 5% of
    1/(μ−λ) and utilisation within 2 percentage points of λ/μ at 100,000 instances for ρ = 0.5 and
    ρ = 0.8;
  - same seed gives an identical serialized result; a different seed gives a different one;
  - every refused construct yields its diagnostic and no result; cancellation and budget exhaustion
    return errors; scenario mutations roll back on failure and undo exactly.
- `apps/processcraft-cli/tests/cli.rs`: `simulate --json` output equals the core result for the same
  seed; MCP `query` returns the same bytes; a GUI unit test asserts the worker result equals a direct
  `run` for the same snapshot.
- `simulation_qualify.rs` records run time and peak memory for 100,000 instances on the medium
  fixture and at the event ceiling, on a declared machine.

**Acceptance:** the analytical checks above pass with the stated tolerances; GUI, CLI and MCP return
the same result for the same seed on the same build; the evidence file states that reproducibility
is claimed for one build on one platform, not across platforms (see Risks).

## Task M7.2: Visio import

**Own:** new `crates/processcraft-io/src/visio.rs`; `crates/processcraft-io/src/{native,lib}.rs`
(shared bounded ZIP helper); `apps/processcraft/src/{effects.rs,app/shell.rs}`;
`apps/processcraft-cli/src/{args,run}.rs`; new `tests/fixtures/visio/`.

- [ ] Fixtures first. Collect `.vsdx` files drawn with the Visio BPMN stencil, each with a sibling
  manifest (provenance, licence, SHA-256, Visio version, expected element counts). Every mapping
  rule below is confirmed against these files before it is implemented; the property and master
  names in this plan are expectations to verify, not facts. See Open decision 5 for who supplies
  the files.
- [ ] Extract the archive checks of `native::load_with_limits` into one crate-private function
  (entry path safety, Stored/Deflated only, no duplicates, per-entry and expanded caps, entry
  count) and use it from both readers. Visio caps: 64 MiB compressed, 256 MiB expanded, 2,048
  entries, 32 MiB per XML part.
- [ ] `visio::prepare_import(bytes, source_name) -> Result<PreparedVisioImport, ImportError>`: parse
  `visio/pages/pages.xml`, each page part and `visio/masters/*` with `quick-xml` under the same
  depth, element-count and attribute limits as `bpmn.rs` (`MAX_DEPTH`, `MAX_ELEMENTS`,
  `MAX_ATTRIBUTES_PER_ELEMENT`). No external entity, relationship target outside the package or
  embedded object is followed.
- [ ] Mapping, each rule with a fixture:
  - shape → element by its master's universal name and the stencil's shape data (task, gateway and
    event type, collapsed or expanded sub-process, data object, data store, text annotation, group,
    pool and lane);
  - text → `name`; shape data that has a BPMN meaning in the qualified profile → the typed field;
  - connectors → sequence flow, message flow or association by master, with source and target from
    the page's `Connect` records; a connector glued at one end only is dropped and reported;
  - lane and pool membership from Visio container relationships, falling back to the shape centre
    lying inside the lane rectangle (reported as approximated);
  - geometry: inches to pixels at 96 per inch, y axis flipped against the page height, pin and
    local pin resolved to top-left bounds; connector paths are **not** translated — flows are routed
    by the existing router and reported as approximated.
- [ ] One `BpmnDocument` per file, one process and one `Diagram` per page; the document is
  `Capability::Editable` and passes `ProjectData::validate()` or the import fails as a whole.
- [ ] `VisioImportReport { mapped, approximated, dropped }` lists every shape and property in one of
  the three groups with its page and shape ID. Committing the import requires acknowledging a
  non-empty `dropped` list (GUI prompt; CLI `--acknowledge-dropped`), as exports require
  acknowledgement of omissions today. The original `.vsdx` bytes are kept as a project `Resource`
  when they fit `RESOURCE_LIMIT`, so nothing is lost by importing (see Open decision 6).
- [ ] Callers: CLI `import-visio <INPUT.vsdx> --out <FILE.processcraft> [--overwrite]
  [--acknowledge-dropped]`, and `info` recognises `.vsdx`; desktop "Import Visio…" in
  `interchange_ui` through `Effects::import` with the same generation and draft checks as BPMN
  import, committing `AddResource` + `CreateDocument` in one batch.

**Tests:** new `crates/processcraft-io/tests/visio.rs`: per fixture, element and flow counts, names,
lane membership, bounds within one pixel of the expected values, and the exact report; refusal of
oversized, nested-archive, path-traversal and non-Visio ZIP inputs (`crates/processcraft-io/tests/
bpmn_limits.rs` is the model); a non-BPMN drawing imports nothing and reports every shape as
dropped; the imported document exports to BPMN that validates against the pinned XSD.
`apps/processcraft-cli/tests/cli.rs`: `import-visio` exit codes and the acknowledgement gate.

**Acceptance:** every fixture imports to an editable document whose report matches its manifest;
no shape is absent from the report; hostile archives are refused before any model is built.

## Task M7.3: Qualification and evidence

- [ ] Quality gates from `CLAUDE.md`; new `docs/implementation/m7-evidence.md` with the analytical
  results and tolerances, timing and memory, the supported and refused construct lists, the Visio
  fixture table and the versions of Visio that produced the fixtures.
- [ ] Update `README.md`, `CLAUDE.md` and the roadmap status. State plainly that simulation output
  is an estimate under the entered assumptions.

## Risks

| Risk | Mitigation |
|---|---|
| Users read simulation output as a forecast | Results panel and exports always show the assumptions, seed and instance count; wording reviewed in M7.3 |
| Floating-point library differences change samples across platforms | Durations rounded to milliseconds; claim limited to one build and platform; cross-platform equality is not promised |
| Unsupported constructs silently skipped | `prepare` refuses with per-element diagnostics; no partial runs |
| Missing inputs filled with invented defaults | Missing durations need explicit acknowledgement; missing probabilities are refused |
| Long runs block the UI or an MCP client | Worker thread with cancel; event budget; MCP query bounded by the same limits |
| Visio stencil names or shape data differ by Visio version or language | Match on universal names; fixtures from at least two versions; unknown shapes are reported, never guessed |
| Visio containers do not give reliable lane membership | Geometric fallback, reported as approximated |
| A malicious `.vsdx` (zip bomb, path traversal) | Shared bounded ZIP helper with caps checked before parsing |

## Open decisions

1. **Where scenario inputs live.** The roadmap says attributes. Recommended: scenarios are
   project-level objects (several per process, compared side by side) whose activity duration and
   cost default to the M5.1 `Duration` and `Cost` attributes. Alternative: a single scenario per
   model stored only in attributes, with what-if limited to comparing checkpoints.
2. **Core module or new crate.** The roadmap leaves it open. Recommended: a module in
   `processcraft-core`; split only if build time or size shows a need.
3. **Inclusive gateways.** Recommended: refuse in M7. Alternative: support inclusive splits with
   independent branch probabilities and a join that waits for the activated branches; it roughly
   doubles the engine's state handling and test matrix.
4. **Calendars.** Recommended: one weekly calendar per scenario for arrivals and all pools.
   Alternative: per-pool calendars and holidays.
5. **Visio fixtures.** Authoring them needs a Visio licence with the BPMN stencil. Recommended: the
   user supplies three to five drawings they are entitled to publish under CC0 or CC BY. Without
   real fixtures M7.2 should not start; hand-built archives would not prove compatibility.
6. **Keep the original `.vsdx` in the project.** Recommended: yes, as a `Resource`, so the import is
   loss-free at project level; it increases file size by the drawing's size. Alternative: keep only
   the name and hash in `Provenance`.
7. **Clarify the non-goal.** `solution-design.md` §1.2 excludes a "process execution engine".
   Simulation as bounded here does not execute anything, so no amendment is required; recommended:
   add one clarifying sentence to §1.2 when M7 is accepted, to prevent later drift toward expression
   evaluation.
8. **Persist results.** Recommended: no; results are recomputed from scenario, model and seed.
   Alternative: store the last result per scenario for the handbook, at the cost of stale data.
