---
name: processcraft-review-conventions
description: Review an existing ProcessCraft model against the modeling-conventions profile through the local MCP server, fix only the findings that are unambiguous, and report the rest. Use when the user asks to check, clean up or review a model's naming and structure.
---

# Review conventions

Read [../README.md](../README.md) first: tools, change protocol, ID rules and the global must-nots.
Fenced `json` blocks below carry an info string; the pack's test validates each one against the
live schemas.

## Inputs

- A session on the project to review (`--in <file>`, or attached to the desktop app).
- Which profile to review against: the profile stored in the project, or "the recommended profile"
  (the user must say so; setting a profile changes project content).
- Permission to edit. If the user only asked for a review, stop after step 4 and report.

The eight rules: `labelRequired`, `activityVerbObject` (word-list heuristic, see below),
`gatewayFlowsLabelled`, `noImplicitSplitJoin`, `singleStartEnd`, `maxElements`,
`requiredAttributes`, `dictionaryTerms`. Findings have codes such as `conventionActivityVerbObject`
and `conventionRequiredAttribute`, a `severity`, the `documentId` and the `elementId` to act on.

## Tool sequence

1. `inspect`: note `sessionId` and `generation`.
2. `query` `diagnostics`: the findings under the stored profile. Page until `nextOffset` is `null`.
3. If the user asked for the recommended profile, `apply` `setConventionsProfile` with it (see the
   example), then `query` `diagnostics` again. If the project has no stored profile, only the
   built-in advisory findings appear until a profile is set.
4. Group the findings by code and element. For each, decide: fix, or report.
   - Fix when the correction follows from the model itself: an activity label that is a noun
     phrase of a step that is clearly "Notify requester" and not "Requester notification"; a
     gateway flow whose sibling already carries "Approved" and whose target is a rejection.
   - Report, do not fix: a missing attribute value, a role that is not in the dictionary, a model
     over `maxElements`, a missing end event, anything where the correction is a business decision.
5. `apply` the fixes as one batch (`rename` for labels and flow names), with `expectedGeneration`
   from the last result.
6. `query` `diagnostics` and `validate` again. The answer is the before and after counts, what you
   changed (old and new text), and what remains with the reason.

`activityVerbObject` checks the first word against a built-in verb list. It can flag a correct
label whose verb is not in the list and miss a bad label that happens to start with a listed
word. Say so in the report, and do not rename a label the user wrote deliberately just to satisfy it.

## Worked example

Review against the recommended profile (the stored profile checks only `activityVerbObject`):

```json tool=apply
{
  "sessionId": "00000000-0000-4000-8000-000000000000",
  "requestId": "review-profile",
  "expectedGeneration": 1,
  "request": {
    "type": "batch",
    "mutations": [
      {"type": "setConventionsProfile", "profile": {"rules": [
        {"type": "labelRequired", "severity": "warning", "kinds": ["Activity", "Event", "Lane", "Participant"]},
        {"type": "activityVerbObject", "severity": "warning", "verbs": []},
        {"type": "gatewayFlowsLabelled", "severity": "warning"},
        {"type": "noImplicitSplitJoin", "severity": "warning"},
        {"type": "singleStartEnd", "severity": "warning"},
        {"type": "maxElements", "severity": "warning", "limit": 50},
        {"type": "requiredAttributes", "severity": "warning"},
        {"type": "dictionaryTerms", "severity": "warning"}
      ]}}
    ]
  }
}
```

Read the findings:

```json tool=query
{"kind": "diagnostics", "offset": 0, "limit": 50}
```

A finding row (use `documentId` and `elementId` exactly as returned):

```json output
{"code": "conventionActivityVerbObject", "documentId": "44444444-4444-4444-8444-444444444444", "elementId": "notify", "severity": "warning", "message": "Activity label should be a verb followed by an object. This is a word-list heuristic; the first word is checked against the verb list."}
```

Fix it. The activity currently reads "Requester notification"; its lane and neighbours show it is
the step that tells the requester the outcome:

```json tool=apply
{
  "sessionId": "00000000-0000-4000-8000-000000000000",
  "requestId": "review-fix-notify",
  "expectedGeneration": 2,
  "request": {
    "type": "batch",
    "mutations": [
      {"type": "rename", "document_id": "44444444-4444-4444-8444-444444444444", "element_id": "notify", "name": "Notify requester"}
    ]
  }
}
```

## On GenerationConflict

Nothing was applied. `inspect`, re-run `query diagnostics`, and look at the elements you meant to
rename: a person may already have fixed or renamed them. Rebuild the batch from the fresh findings,
send it under a new `requestId`. If a finding vanished, drop that fix.

## Must not

- Rename, delete or relabel to silence a finding when the right text is a business decision.
- Change the stored profile (severities, limits, verb list) without being asked; a profile is the
  team's agreement, not the agent's.
- Take an element ID from memory or from a label: use the `elementId` of the finding.
- Touch elements that have no finding, or fix a finding by adding an attribute value you invented.
- Claim the model "passes BPMN validation" or is conformant when the diagnostics are empty.
  An empty list means no finding under the profile in use.
