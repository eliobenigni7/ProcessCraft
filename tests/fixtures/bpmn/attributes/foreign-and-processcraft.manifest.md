# Custom attribute interchange fixtures (M5.1b)

Both files are synthetic, authored for ProcessCraft on 2026-11-13; no external model content.
License: CC0 1.0 Universal (https://creativecommons.org/publicdomain/zero/1.0/).

## `foreign-and-processcraft.bpmn`

- SHA-256: `da138649f8c4dfedda4e567c46735233db89756a0d4d81489b9f51f3b0295da1`
- Expected import mode: editable (`prepare_import` qualifies it; original source retained).
- Content: one user task whose `extensionElements` holds two Camunda-style foreign elements
  followed by a well-formed `pc:attributes` (currency, duration, enum, dictionaryRef). The
  `pc:attributes` is lifted into `PreparedImport.attributes` and removed from the stored XML
  content; the foreign elements stay in order and are written back before a new `pc:attributes`.
- Normalization on import: `12.50` is stored as `12.5`, `PT90M` as `PT1H30M`. The first `pc:ref`
  names an entry ID that no project has, so a fresh project stores it as a literal.

## `foreign-and-processcraft-malformed.bpmn`

- SHA-256: `e03b3bf575d0c3c9e3aa5f2feacbcac79a80e185394218bbf77247223e2d4454`
- Expected import mode: editable, with a limitation naming the task.
- Content: the same file with `type="bogus"` on one attribute. The whole `pc:attributes` element
  stays in the task's extension content (preserved, not reinterpreted); no definitions or values
  are lifted.
