---
name: processcraft-fill-attributes
description: Fill custom attribute values (cost, risk, duration, owners and similar) on the activities of an existing ProcessCraft model from material the user supplies, through the local MCP server, then confirm that no required attribute is left empty. Use when the user has a table, note or document with values to enter.
---

# Fill attributes

Read [../README.md](../README.md) first: tools, change protocol, ID rules and the global must-nots.
Fenced `json` blocks below carry an info string; the pack's test validates each one against the
live schemas.

## Inputs

- A session on the project (`--in <file>`, or attached to the desktop app).
- The source material: which activity gets which value.
- The document ID and process ID: `query documents` (README) lists them.
- Whether new attribute definitions may be created. Without that permission, fill only existing
  ones. A note that mentions an attribute the project does not define ("risk") is not permission
  to create it; report that it is missing and what you would create.

Matching a source name to an activity: the activity names as the model spells them (`table`
`name`, or the `key.xml_id`) are the only names that exist. An exact match, ignoring case and
surrounding spaces, is a match. A near match ("Approval" against "Approve request", "Ordering"
against "Place order") is a guess: do not set a value on it. List the source row, the closest
model names and why you did not choose, and ask; in an autonomous run, leave the value unset
and say so. The same holds for attribute names against definitions.

Value formats (canonical, checked by the core): `Text` any string; `Number`, `Currency` a decimal
string such as `"40.5"` (never a float); `Duration` ISO 8601 `PnDTnHnMnS` such as `"PT2H30M"` (`P1D`
is 24 elapsed hours, not a working day); a note's words map as 45 minutes to `PT45M`, 1 hour to
`PT1H`, 2 days to `P2D`, 1 hour 30 minutes to `PT1H30M`; `Enum` one of the definition's `options`; `Date`
`YYYY-MM-DD`; `Url` text; `DictionaryRefs` a list of `{"Entry": "<dictionary id>"}` (a
dictionary entry, taken from `query dictionary`) or `{"Literal": "<text>"}` (free text that is not
in the dictionary; it is stored and shown but cannot be reused or checked). The wire form is
`{"<Type>": value}`, for example `{"Currency": "12.5"}` or `{"Enum": "Low"}`.

## Tool sequence

1. `inspect`: note `sessionId` and `generation`.
2. `query` `attributes` (needs `document_id`): the definitions (with `id`, `value_type`, `options`, `unit`, `required`,
   `applies_to`, `usage_count`) and the values that exist. Rows appear only for elements that
   already have a value.
3. `query` `diagnostics`: `conventionRequiredAttribute` findings name the activities whose required
   attribute is empty (when the stored profile contains `requiredAttributes`).
   `query` `table` (needs `document_id` and `process_id`) lists every activity with the attributes
   it has, if you need the full list. An empty diagnostics list means no such finding under the
   stored profile; it does not prove every value is filled (check `table`), and if the project
   stores no profile the finding cannot appear at all.
4. Map source rows to `(elementId, definition id, value)`. Take element IDs from `key.xml_id` and
   definition IDs from the `definitions` list. For a new definition, mint a fresh UUID.
5. `apply` one batch: `createAttributeDefinition` first (only if allowed), then one
   `setAttributeValues` per element. A value you set replaces the existing one for that
   definition; `"value": null` clears it. If an element already has a different value from the
   one in the source, do not guess which is right. Overwrite only when the source itself says the
   value changed ("changed to 30", "now 120", "was 100, is 120"); "changed to X" is enough, no
   further confirmation is needed. List every overwrite (old and new) in the report. When the
   source gives a different value without such wording, keep the stored value and report the
   discrepancy (stored value, source value, element). "Different" is not the same as "changed".
   If nothing in the source can be placed, `apply` nothing and report why; an empty result with
   reasons is correct.
6. `query` `attributes` and `query` `diagnostics`, then `validate`. Report the values set, the
   source rows you could not place, and any required attribute still empty.

## Worked example

Source: "Review request costs 100 EUR (already entered). Notify requester costs 5 EUR. Rate the
risk of all three activities: review Medium, order High, notify Low." The model has a required
`Cost` attribute (Currency, EUR), and `query diagnostics` reports it missing on `notify`. The
project was opened from a file, so the first `inspect` shows generation 0.

```json tool=query
{"kind": "attributes", "document_id": "44444444-4444-4444-8444-444444444444", "offset": 0, "limit": 50}
```

```json tool=query
{"kind": "diagnostics", "offset": 0, "limit": 50}
```

The user allowed a new `Risk` enum, so create it and set all values in one batch:

```json tool=apply
{
  "sessionId": "00000000-0000-4000-8000-000000000000",
  "requestId": "fill-1",
  "expectedGeneration": 0,
  "request": {
    "type": "batch",
    "mutations": [
      {"type": "createAttributeDefinition", "definition": {"id": "6a000000-0000-4000-8000-000000000011", "name": "Risk", "description": "Operational risk of the activity.", "value_type": "Enum", "unit": null, "options": ["Low", "Medium", "High"], "dictionary_kind": null, "applies_to": ["Activity"], "required": false}},
      {"type": "setAttributeValues", "source": {"document_id": "44444444-4444-4444-8444-444444444444", "xml_id": "notify"}, "values": [
        {"definition_id": "44444444-4444-4444-8444-000000000010", "value": {"Currency": "5"}},
        {"definition_id": "6a000000-0000-4000-8000-000000000011", "value": {"Enum": "Low"}}
      ]},
      {"type": "setAttributeValues", "source": {"document_id": "44444444-4444-4444-8444-444444444444", "xml_id": "review"}, "values": [
        {"definition_id": "6a000000-0000-4000-8000-000000000011", "value": {"Enum": "Medium"}}
      ]},
      {"type": "setAttributeValues", "source": {"document_id": "44444444-4444-4444-8444-444444444444", "xml_id": "order"}, "values": [
        {"definition_id": "6a000000-0000-4000-8000-000000000011", "value": {"Enum": "High"}}
      ]}
    ]
  }
}
```

A `Duration` definition (unit `null`, no options) is created like this, and is used with values
such as `{"Duration": "PT45M"}`:

```json tool=request
{
  "type": "batch",
  "mutations": [
    {"type": "createAttributeDefinition", "definition": {"id": "6a000000-0000-4000-8000-000000000012", "name": "Duration", "description": "Time the activity takes.", "value_type": "Duration", "unit": null, "options": [], "dictionary_kind": null, "applies_to": ["Activity"], "required": false}}
  ]
}
```

An element whose kind a definition does not apply to (`applies_to`), a value outside `options`, a
malformed decimal or an unknown dictionary entry makes the whole batch fail with
`InvalidParams(...)` and nothing is changed; read the message, correct the batch, and resend it
under a new `requestId`.

## On GenerationConflict

Nothing was applied. `inspect`, then `query attributes` and `query diagnostics` again: values may
have been entered by someone else in the meantime. Rebuild the batch so that it does not overwrite
those values, and resend under a new `requestId`.

## Must not

- Invent a value, a unit or a currency that the source does not give; leave it empty and report.
- Guess a definition ID, an element ID or a dictionary entry ID; use the query results.
- Overwrite an existing value silently, or clear one, unless the source says to.
- Create definitions without permission, or change a definition's `required`, `options` or type
  to make values fit.
- Store a float: write decimals as strings.
- Claim that the model is complete or BPMN-conformant because no `requiredAttributes` finding remains.
