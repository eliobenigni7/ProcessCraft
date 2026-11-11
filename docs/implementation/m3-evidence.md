# M3 partial-delivery evidence

This records the temporary partial close requested by the user on 2026-11-11. M3 and full BPMN 2.0.2 modeling/interchange remain unfinished. Earlier checks were collected while other work was in progress; their exact limits remain relevant.

## PDF export

`cargo run -p processcraft-cli --example pdf_qualify --locked --offline` generated Light, Dark and four-page tiled fixtures in `target/m3-qualification/pdf`. `python scripts/check_pdf.py` independently parsed PDFs with pypdf and Poppler, rasterized them with `pdftoppm` at 96 dpi, and wrote `report.json`. It checked A4 landscape page dimensions and count, rejection without acknowledgement and above 100 pages, no image XObjects or embedded fonts in the generated fixture resources, and no extractable text. The explicit outline notice was present. Comparison with the same scene PNG gave mean absolute RGB differences of 1.893/255 (Light) and 1.714/255 (Dark); four tiled pages were 0.071–0.780/255, excluding two pixels at tile seams. Latin accents, Arabic and CJK were visually inspected in rasterized samples. These comparisons do not establish accessibility, text searchability or correctness for all files. PDF text is outlines and is not selectable, searchable or tagged.

The tiling clips at page boundaries. Visual inspection showed a shape and its text split between pages; the feature does not keep BPMN elements together or repeat page context. The two-pixel seam exclusion means parity at the exact seam was not measured.

## Auto-layout

`cargo test -p processcraft-core --test layout --locked --offline` covers deterministic repeated preview, a branch/join, cycle and self-loop, one pool with lane and unlaned nodes, labels and explicit styles, event/gateway border endpoints, malformed/missing/duplicate DI and cancellation, proposal tampering, exact session apply/no-op/undo/redo, 400 nodes, and 500-edge refusal/acceptance without mutating the source. Tests are fixture-scoped; they do not establish all BPMN structures or imports.

A bounded straight-route fast path handles the connected 400-node / 399-edge linear fixture without changing the shared manual router. A second fixture adds 101 non-adjacent edges (400 nodes / 500 edges); it was **refused with `routing`**, without changing the source. Complex diagrams can be refused on collision, crossing, shared channel, obstacle limit or the 500,000-unit routing-work budget. The shared corridor-search helper still limits non-straight routing to 256 obstacles; 400 connected nodes therefore does not imply arbitrary 400-node layouts are supported. Loop/nested and newly modelled BPMN kinds require separate qualification; only the explicitly checked subset is eligible.

On this machine (Windows 11 Pro 10.0.26300, AMD Ryzen 5 5600X, 6 cores / 12 logical processors), one debug-build test-process wall-clock measurement for the 400-node chain plus the attempted 500-edge fixture was 293 ms. This includes test process startup and both cases, is a single sample, is not a release benchmark or a guarantee on other hardware, and does not establish the plan's worst-case two-second target.

## Renderer

At the nested-renderer checkpoint, `cargo test -p processcraft-render --locked --offline` passed the renderer suite and `cargo clippy -p processcraft-render --all-targets --locked --offline -- -D warnings` passed. Tests cover nested semantic IDs, a subprocess child plane, expanded/collapsed visibility in full scene and viewport, child flow labels, standard/multi-instance markers, event hit shapes and explicit color preservation. Parsing, schema preservation and end-to-end UI navigation remain separate qualifications. A missing `isExpanded` value remains missing; the renderer reports its ambiguity rather than silently asserting a normative default.

## Nested geometry move

`cargo test -p processcraft-core --test geometry --locked --offline` passed 16 tests after nested move changes. The targeted fixtures verify a subprocess drag moves child shapes, translates its internal route and explicit edge-label bounds in one undoable batch, preserves label style, repairs a directly moved nested child route, and refuses conflicting parent/child deltas without mutation. The existing direct multi-selection policy keeps manual interior bends. Connected data associations and conversation links are refused when their endpoints move until their geometry handling is qualified. These tests do not cover every nested BPMN family or native pointer interaction.

## Checkpoint and native limits: synthetic release run

Reproduce with `CARGO_INCREMENTAL=0 cargo run -p processcraft-cli --example m3_qualify --release --locked --offline -- --large` (allocates roughly 256 MiB of resource bytes and can use substantially more working memory). Generated synthetic archives and measurements are in `target/m3-qualification/checkpoint`; no user files were used. Results below are one run on Windows 11 Pro 10.0.26300, AMD Ryzen 5 5600X, 6 cores / 12 logical processors; they are not a statistical benchmark.

- A 32 MiB resource passed model validation; 32 MiB + 1 byte was rejected. Twenty checkpoints were accepted and survived native reopen; the 21st was rejected without changing content identity. The 20-checkpoint small fixture had 20 undo entries.
- Eight distinct, highly compressible synthetic resource blobs totaling 268,369,920 bytes (256 MiB minus 64 KiB), plus project/checkpoint metadata, were saved and reopened with the frozen reference and content identity intact. The archive was 1,226,572 bytes. Write took 639.7 ms and reopen 646.8 ms; the entire large-fixture phase took 2,805.9 ms. The test did not locate the exact largest expanded-byte payload; eight full 32 MiB blobs plus metadata were rejected by the expanded-size validator. The 64 KiB gap is a deliberately conservative near-limit case.
- Independent Python `zipfile` inspection found eight resource entries for those eight unique hashes, shared between current and frozen content rather than duplicated for the checkpoint. The one-resource 32 MiB fixture similarly had one blob entry and retained its reference after reopening. Compressed sizes reflect repeated-byte test content and do not predict real-file sizes or qualification of the separate 128 MiB archive-size limit.
- The near-expanded session reported **zero undo entries** after creating the checkpoint and reference because history has a 128 MiB budget. The operation itself succeeded; an unconditional promise that every checkpoint operation is undoable would therefore be inaccurate near the resource ceiling. The small 32 MiB fixture retained two undo entries.

A subsequent release run after the undo-budget guard changed the near-limit project after its checkpoint and attempted restore: restore was rejected with `operation cannot fit in the undo/redo history budget`; content identity and generation stayed unchanged. That run recorded 635.5 ms write, 666.2 ms reopen, and 3,509.7 ms for the large-fixture phase. The previous measurements remain a separate single run, not an aggregate benchmark.

The labelled two-lane operations fixture now produces a layout proposal: existing external shape labels are routing obstacles, and multiple event/gateway edges use different cardinal attachment points. This does not remove the documented bounded-routing refusals for more complex diagrams.

Gateway renderer fixture tests verify default-flow marking by the source gateway's semantic flow ID (not the DI edge ID), and compare marker-present/absent SVG, PNG, PDF and viewport output. Event-based exclusive/parallel and instantiating/non-instantiating fixture vectors differ as expected. These are rendering checks, not an assertion that all gateway XML attributes or behaviors are supported.

## Native desktop review and schema 3 persistence

An actual debug Windows desktop session opened the synthetic schema-2 `target/ui-production/operations-ui.processcraft`, reported migration/Save As, and saved a separate schema-3 `target/m3-qualification/native-ui-review.processcraft`. UI Automation exercised posting an English comment on `review`, refusal to save an unposted comment draft, checkpoint creation, local-reference selection and frozen-content viewing. Renaming the activity changed the comparison to one renamed element and zero property/diagram changes. Explicit checkpoint restore, Undo and Redo were then invoked. Independent ZIP/JSON inspection confirmed the restored activity name, posted comment, reference ID and separate checkpoint payload. This evidence is specific to that synthetic project and build; it does not qualify all new BPMN property controls.

The native PDF flow displayed outline and pagination notices, required explicit acceptance, then wrote `target/m3-qualification/native-ui-review.pdf`. Independent pypdf parsing found one page and no extractable text, consistent with the disclosed outline policy. Shared-scene raster checks are recorded separately above.

Light, Dark and Dark at UI 200% were visually inspected. At 200%, vertical wheel scrolling brought Restore and the protected disabled Delete-reference action into the window. Inspection captures were saved initially in the temporary directory, with retained copies in `target/m3-qualification/ui/`. They show actual native controls, not browser mockups. Later collaboration/data and activity-wave controls require a separate native pass.

## Comparison and history accounting

`checkpoint_compare` verifies that semantic renames, property edits, DI changes and project/dictionary/resource/comment/link context are reported separately without changing the frozen snapshot. Semantic descendants are compared by ID instead of classifying a child rename as an ancestor property change. The desktop caches the structured comparison by generation/checkpoint and paginates each displayed group with 100 entries per section; structured CLI/MCP results retain their existing response-size ceiling.

History now charges the entire typed BPMN payload through a streaming JSON-size meter in addition to fixed allocated capacities, once per shared document. The four-times serialized-size allowance is intentionally conservative and can shorten available history. Targeted history tests verify large typed metadata is charged and shared document/resource bytes are not charged twice. Restore/layout preflight both previous and candidate states so a successful operation retains the promised Undo/Redo path; refusal changes neither content, generation nor existing history.

## Final partial-delivery checks and remaining work

The final workspace regression passed **326 tests across 30 test targets** with `cargo test --workspace --all-targets --all-features --locked --offline`; its complete output is retained in `target/m3-qualification/workspace-tests.log`. Formatting and workspace Clippy with `-D warnings` passed. The inventory checker reproduced 580 XSD declarations; that number is an inventory, not a coverage percentage. The I/O interchange suite contains 35 tests after the last activity and generated-lane-set changes.

Independent reviews identified comparison detail/pagination gaps and a possible lane-set export problem. The comparison now exposes field-level before/after values, typed navigation/dictionary link differences, moved shapes and rerouted edges, and paginates frozen model/dictionary/comment content. A genuine egui pointer test with 101 frozen comments reaches page 2. Generated lane-set IDs now share a deterministic collision-avoiding allocator across root, subprocess and child lane sets. The generated-ID collision fixture was reimported and independently XSD-validated. The reviewer could not establish whether the earlier nested-lane path dropped content or already refused it; do not record that original suspected loss as a reproduced failure.

A UI camera regression test verifies preview entry/exit preserves editor pan and zoom. Unit-test UI recovery now uses a private temporary folder rather than reading or writing the desktop user's recovery store.

Full coverage remains open: complete event/reference/namespace and executable-model metadata, complete choreography/conversation families, all property/DI edit paths, semantic rules and independent external consumer qualification. The [normative gap crosswalk](../research/bpmn-2-conformance-gap.md) and [expanded plan](../superpowers/plans/2026-11-11-processcraft-m3-implementation-plan.md) define the continuation. Source preservation and XSD acceptance do not close those gates.

## Release close-out

The normal release build of `processcraft.exe` and `processcraft-cli.exe` completed successfully. Byte sizes and SHA-256 hashes are retained in `target/m3-qualification/build-manifest.json`.

A final native run of this release opened the schema-3 review fixture, prepared the labelled two-lane auto-layout preview, applied it, invoked Undo then Redo, and saved a separate `native-layout-review.processcraft`. Independent ZIP/JSON inspection verified changed DI with unchanged process semantics, comments, checkpoint catalog and reference. The release CLI reopened and validated the result with no warnings. The Collaboration and data window was opened and captured; complete per-form native authoring qualification remains open. Release captures are in `target/m3-qualification/ui/processcraft-m3-release-*.png`. The application was closed after the synthetic test reached Saved state.

No further feature work is active at this user-requested partial close. Full M3 completion has not been claimed.
