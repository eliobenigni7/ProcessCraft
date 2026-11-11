# Interoperability process fixture

Synthetic BPMN 2.0.2 fixture authored for ProcessCraft qualification on November 11, 2026. Released under CC0-1.0; no third-party diagram or user content is included.

- Source: `interop-process.bpmn`
- SHA-256: `4370d981ed072f1ca4ba1b321a295f7feb5de30f3c83966bc310349cfadb89d4`
- Scope: one process, start/task/end, two sequence flows, complete shape/edge DI, optional diagram name, and namespace-qualified local DI references.
- Schema: pinned local OMG BPMN 2.0.2 XSD. Source schema validation passed.
- Reproduce native save/reopen and actual XML export with `cargo run -p processcraft-io --example identity_qualify --locked --offline -- <absolute-output-directory>`.
- External-tool results are separate evidence under `target/m3-qualification/external/`; fixture validity does not itself establish interoperability or full conformance.
