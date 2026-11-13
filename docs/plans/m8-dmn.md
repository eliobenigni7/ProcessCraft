# ProcessCraft M8 Implementation Plan — Decision tables (DMN)

**Goal:** Deliver M8 of the [roadmap](roadmap.md): DMN decision tables as a separate document kind,
linked from business rule tasks, evaluated locally against example inputs with a bounded S-FEEL
subset, and exchanged as DMN XML under the same preservation rule as BPMN.

**Status:** Planned; not started. The amendment text below was accepted on 2026-11-13 (roadmap,
"Decisions taken"); writing it into `solution-design.md` is the first step when M8 starts, together
with confirming the DMN version against OMG.

## Scope amendment required

`docs/design/solution-design.md` line 67 (§1.2 Initial exclusions) currently reads:

> No process execution engine, process mining, distributed repository, authenticated approvals,
> real-time collaboration, scripting language, plugin marketplace, cloud synchronization, generative
> AI provider, or full DMN editor.

and line 69 continues:

> DMN and richer notations can become separately specified document kinds later. They do not enter
> the first BPMN schema through speculative generic abstractions.

M8 touches two of the listed exclusions: "full DMN editor", and — because a decision table is
evaluated — "scripting language". The roadmap names only the first; the second needs the same
explicit treatment.

**Amendment text** (accepted; written into `solution-design.md` when M8 starts):

1. In the sentence at line 67, replace "or full DMN editor" with "or DMN beyond the decision-table
   scope of §1.2.1".
2. Add after line 69:

   > #### 1.2.1 Amendment (M8): DMN decision tables
   >
   > DMN decision tables are a second document kind, specified separately from BPMN and stored
   > beside BPMN documents in the same project. In scope: decision tables with hit policies, typed
   > input and output columns, DMN XML import and export under the preservation contract of §6, a
   > link from a BPMN business rule task to a decision, and local evaluation of one decision table
   > against example inputs entered by the user.
   >
   > Still excluded: editing decision requirements diagrams as a notation; boxed expressions other
   > than decision tables (literal expressions, contexts, invocations, functions, relations, lists);
   > business knowledge models and decision services; FEEL beyond the documented S-FEEL subset;
   > evaluating a decision as part of running or simulating a process.
   >
   > Evaluating S-FEEL unary tests is not a scripting language in the sense of §1.2: the subset has
   > no statements, no user-defined functions, no input or output, and a fixed evaluation budget.

3. In §6 add one sentence that the three import modes and the "preserved or refused" rule apply to
   DMN XML as they do to BPMN XML.

## Architecture

A new `DmnDocument` type in `processcraft-core`, stored in `ProjectData.decisions`, changed only
through new `Mutation`s. Cell contents are stored as their source text; parsing and evaluation are
pure functions in core. `processcraft-io` gains a bounded DMN XML reader and writer. The desktop gets
a table editor and a test panel. No new crate; no new dependency is assumed.

**Authority:** [roadmap](roadmap.md) §M8; [solution design](../design/solution-design.md) §1.2 as
amended, §5.3, §6, §7, §8.2, §17. Depends on [M5](m5-analyst-foundations.md) (decimal and date
validators, the ProcessCraft extension namespace, editable table cells) and on M6.2 for navigation.

## Boundaries

In scope: what the amendment lists. Out of scope, and not to be scaffolded: everything the amendment
keeps excluded; static analysis of tables (gaps, overlaps, subsumption); decision table rendering to
SVG/PNG/PDF (the M6 handbook shows tables as HTML); a generic "document kind" abstraction over BPMN,
process maps and DMN — each kind stays a concrete type.

## Findings from the current code that shape this plan

1. **BPMN 2.0.2 has no standard reference from a business rule task to a decision.**
   `ActivityProperties.implementation` (`bpmn_activity.rs`) is a free string. Tools use vendor
   attributes. The link is therefore project content, exported in the ProcessCraft extension
   namespace introduced in M5.1.
2. **`Capability` (`Editable`, `RetainedExtensions`, `PreservedSource`) and `Provenance` are generic
   enough to reuse for DMN documents**; `bpmn_xml::XmlContent` can hold retained foreign content.
3. **`tests/fixtures/bpmn20-schema/` is the precedent for pinned OMG schemas**, with
   `scripts/check_bpmn20_xsd.py` as the independent validator.
4. **The roadmap names DMN 1.6.** This plan could not confirm offline that 1.6 is a formal OMG
   release rather than a beta. See Decision 1.

## Slices

### Task M8.1: Model, commands and native storage

**Own:** new `crates/processcraft-core/src/dmn.rs`; `model.rs`, `commands.rs`, `identity.rs`,
`history.rs`, `queries.rs`, `validation.rs`; `crates/processcraft-io/src/native.rs`.

- [ ] `DmnDocument { id: DocumentId, name, folder, definitions_id, namespace, decisions:
  Vec<Decision>, diagram: Option<DmnDiagram>, provenance, capability }`; `Decision { id, name,
  question, allowed_answers, table: DecisionTable, required_decisions: Vec<XmlId> }`;
  `DecisionTable { hit_policy, aggregation, inputs: Vec<InputClause>, outputs: Vec<OutputClause>,
  rules: Vec<Rule> }`; `InputClause { id, label, expression: String, type_ref, allowed_values }`;
  `OutputClause { id, name, label, type_ref, output_values, default_value }`; `Rule { id,
  input_entries: Vec<String>, output_entries: Vec<String>, description }`. Entries are source text.
  `DmnDiagram` holds only shape bounds and edge waypoints by element reference, enough to write a
  requirements diagram back unchanged.
- [ ] Limits: 500 decisions per document, 64 columns and 1,000 rules per table, 4 KiB per cell.
- [ ] `ProjectData.decisions: Vec<Arc<DmnDocument>>` (serde default, skipped when empty), in
  `FrozenProject`; validation of IDs (unique across documents, maps and decisions), column and
  entry counts per rule, references. Native capability `"dmn-decision-tables-v1"`.
- [ ] Mutations with descriptors: `CreateDmnDocument`, `DeleteDmnDocument`, `SetDmnDocumentProperties`,
  `CreateDecision`, `UpdateDecision` (name and metadata), `DeleteDecision`, `SetDecisionTable {
  document_id, decision_id, table }` (whole-table replace; one cell edit in the GUI is one such
  mutation and one undo step). All refuse non-editable documents.
- [ ] Decision link: `ProjectData.decision_links: Vec<DecisionLink { source: ElementKey, document:
  DocumentId, decision: XmlId }>` and `Mutation::SetDecisionLink`; source must be a
  `NodeKind::BusinessRuleTask`; pruned with the M5.1 helper; `queries::drill_target` (M6.2) gains a
  `Decision` case; a link to a missing decision is a diagnostic.

### Task M8.2: S-FEEL subset and evaluation

**Own:** new `crates/processcraft-core/src/feel.rs`; `dmn.rs`.

- [ ] Parser for input entries (unary tests): `-`; comma-separated alternatives; `not(…)`;
  comparisons `<`, `<=`, `>`, `>=`; intervals with open and closed ends; literals — number, string,
  boolean, `date("…")`, `time("…")` without zone, `date and time("…")` without zone,
  `duration("…")` (days and time only); and a simple name referring to another input. Output
  entries: literals only in the first cut. Recursive descent with depth limit 32. Anything outside
  the subset is a parse diagnostic naming the cell, never a guess.
- [ ] Values: numbers compare as decimals using the M5.1 decimal parser (no `f64`); dates through
  the M5.1 date parser; type mismatches are evaluation errors, not `false`.
- [ ] `dmn::evaluate(table, inputs) -> Result<Evaluation, EvaluationError>` for all hit policies:
  Unique, Any, First, Priority, Rule order, Output order, Collect (list, sum, min, max, count).
  `Evaluation` lists matched rule indexes, outputs and any hit-policy violation (several matches
  under Unique, differing outputs under Any). Work is bounded by rules × inputs; a table with any
  unparsed cell is refused as a whole.
- [ ] `ProjectData::diagnostics()` reports unparsed cells, type errors against the column type and
  entries outside `allowed_values`.

### Task M8.3: DMN XML import and export

**Own:** new `crates/processcraft-io/src/dmn.rs`; `lib.rs`; new `tests/fixtures/dmn/` and
`tests/fixtures/dmn-schema/`.

- [ ] Bounded parser with the limits of `bpmn.rs` (size, depth, element and attribute counts; no
  DTD, entity or external reference). Accept the model namespaces of the DMN versions listed in the
  fixture manifest; record the source version in `Provenance`.
- [ ] `prepare_dmn_import` promotes a file to `Editable` only when every decision's logic is a
  decision table inside the modelled fields and all other content (requirements, diagram, extension
  elements) is represented or retained. Any other boxed expression, business knowledge model,
  decision service or unknown standard content makes the whole document `PreservedSource` with the
  original bytes as a `Resource`: listed in the library, exported byte-for-byte, not editable.
- [ ] `export_dmn` writes canonical XML in the source version's namespace (the pinned version for
  new documents) with an `ExportReport` in the existing retained/transformed/omitted/blocked form.
- [ ] BPMN side: the decision link is written as a `pc:decisionRef` element in the business rule
  task's `extensionElements` (M5.1 namespace mechanism) and read back; vendor attributes are neither
  written nor interpreted.
- [ ] Fixtures with manifest entries: synthetic tables for each hit policy and type; one file per
  accepted DMN version; files with other boxed expressions (expected mode: preserved source); a
  subset of the DMN TCK decision-table cases with their expected results, with licence and
  provenance recorded. Pinned XSD in `tests/fixtures/dmn-schema/` and a
  `scripts/check_dmn_xsd.py` mirroring the BPMN one.

### Task M8.4: Desktop, CLI and MCP

**Own:** new `apps/processcraft/src/app/decision.rs`; `analyst_views.rs`, `shell.rs`, `inspector.rs`,
`effects.rs`; `apps/processcraft-cli/src/{args,run,mcp}.rs`; `apps/processcraft/src/attachment.rs`.

- [ ] Library lists decision documents; opening one shows the decision list and a table editor built
  on the editable `data_table` cells from M5.1: hit policy, add/remove/reorder columns and rules,
  column type, cell text with the parse diagnostic inline. Keyboard-complete.
- [ ] Test panel: one field per input (typed), Evaluate, matched rules highlighted, outputs and
  violations shown. Test inputs are view state, not saved.
- [ ] Inspector of a business rule task: "Decision" picker submitting `SetDecisionLink`; drill-down
  as in M6.2.
- [ ] Import/Export DMN in `interchange_ui` and `Effects`; CLI `import-dmn`, `export-dmn`,
  `query decisions`, `evaluate <INPUT> --document-id … --decision … --inputs <FILE.json> --json`;
  MCP `Query::Decisions` and `Query::EvaluateDecision` in both query enums, DMN in the MCP `Export`
  kinds.

### Task M8.5: Qualification and evidence

- [ ] Quality gates; `docs/implementation/m8-evidence.md` with the supported S-FEEL grammar, the
  per-fixture import mode and round-trip result, TCK cases passed and skipped with reasons, and the
  limits exercised. Update `README.md`, `CLAUDE.md`, roadmap status. No DMN conformance level is
  claimed.

## Tests

- New `crates/processcraft-core/tests/dmn.rs` (model limits, mutations, rollback, undo, checkpoint
  capture/restore, link pruning) and `feel.rs` (grammar accept/reject table, comparison semantics
  per type, depth limit) and `dmn_evaluate.rs` (each hit policy, violations, the TCK subset).
- New `crates/processcraft-io/tests/dmn.rs` (import modes per fixture, import → edit → export →
  re-import equality, preserved source exported byte-for-byte, limits, XSD validation of exports)
  and additions to `native.rs` (capability, round trip).
- `apps/processcraft-cli/tests/cli.rs` (`evaluate` equals the core result; MCP returns the same;
  exit codes). GUI unit tests in `decision.rs` (cell edit is one undo step; invalid cell keeps the
  draft and shows the diagnostic).

## Acceptance

- Every hit policy has a fixture that evaluates to its expected result in GUI, CLI and MCP.
- Each DMN fixture ends in its manifest's expected import mode; editable ones round-trip with
  semantic equality and validate against the pinned XSD; preserved ones export byte-for-byte.
- A business rule task linked to a decision survives native save/reopen and BPMN export/re-import,
  and a deleted decision produces a problem that navigates to the task.
- The TCK subset passes, with skipped cases listed and explained.

## Risks

| Risk | Mitigation |
|---|---|
| Scope creep from S-FEEL to FEEL | Grammar fixed in M8.2 and in the amendment; anything else is a diagnostic |
| Most real DMN files carry requirements diagrams or literal expressions | Minimal typed diagram geometry; other logic kinds fall back to preserved source, counted honestly in the evidence |
| Decimal and date semantics differ from other engines | TCK subset as the reference; differences recorded |
| Namespace differences between DMN versions | One fixture per accepted version; export in the source version |
| Whole-table replace is costly on large tables | Bounded at 64 × 1,000 cells; measure before optimising |

## Decisions (adopted 2026-11-13)

The recommended option of each former open decision was adopted (roadmap, "Decisions taken")
unless stated otherwise below. The alternatives are kept for the record.

1. **DMN version to pin.** The roadmap says 1.6. Adopted: pin the latest *formal* OMG DMN
   release at the time M8 starts and accept the earlier model namespaces on import; confirm whether
   1.6 is formal before pinning its XSD.
2. **Amendment text above**, including the sentence on S-FEEL versus "scripting language":
   accepted.
3. **Output entries.** Adopted: literals only in M8; arithmetic and names in output entries are
   reported as outside the subset. Alternative: add S-FEEL arithmetic, which brings decimal
   arithmetic and rounding rules into core.
4. **Decisions that depend on other decisions.** Adopted: keep `required_decisions` for round
   trip and navigation, evaluate one table at a time. Alternative: chained evaluation.
5. **Decision link in exported BPMN.** Adopted: only `pc:decisionRef`. Alternative: also write a
   vendor attribute such as Camunda's, which means choosing and tracking a vendor format.
6. **TCK use.** Adopted: vendor a small, attributed subset of decision-table cases as fixtures.
   Alternative: synthetic fixtures only.
