# Opening and exporting files

## Your project file

A project is one `.processcraft` file. It holds every document and diagram, the library folders, the
dictionary, attribute definitions and values, the conventions profile, comments and checkpoints.

- **Saves are atomic**: a crash during a save leaves the previous version intact.
- **A lock file** (`<name>.processcraft.lock`) next to the project stops two ProcessCraft windows from
  writing the same file. A second window opens it read-only. The lock file is harmless and may stay.
- **Recovery snapshots** are kept outside the project, so unsaved work can be recovered after a crash.
- **Checkpoints** (**Revisions** tab of the bottom panel) are named versions inside the project that
  you can compare with the current model and restore.

## Opening BPMN files

**File → Open…** or **Import BPMN…** opens BPMN 2.0 XML files from Camunda Modeler, bpmn.io and
other tools. You can also paste BPMN XML onto the canvas. The import report tells you how the
file opened:

| Import mode | What it means |
|---|---|
| Editable | The model fits the supported profile and is fully editable. |
| Editable with retained extensions | Vendor extensions are kept verbatim. Vendor attributes such as `camunda:assignee` do not restrict modeling, but an element they reference cannot be deleted, and vendor elements ProcessCraft does not model block edits that could invalidate them. |
| Preserved source | The original XML is kept byte for byte and exported unchanged. The report names what prevented editing. |

Content ProcessCraft cannot edit is never dropped quietly or reinterpreted. Files are read with size
limits, without DTDs or external entities, and nothing is fetched from the network.

ProcessCraft supports a broad, tested part of BPMN 2.0.2, not all of it. The
[coverage notes](https://github.com/eliobenigni7/ProcessCraft/blob/main/docs/research/bpmn-2-coverage.md)
list it element by element.

## Exporting

**Export** in the document header offers:

| Format | Use it for |
|---|---|
| BPMN | Camunda Modeler, bpmn.io and other BPMN tools |
| draw.io | diagrams.net and wikis that embed it; uses draw.io's own BPMN shapes |
| SVG | web pages and vector editing |
| PNG | slides and documents |
| PDF | printing and sharing |

Images and PDFs are drawn by the same renderer as the canvas, in a light or a dark appearance. With
an overlay on, **Include overlay and legend** adds the heatmap or badges.

Before a BPMN export, ProcessCraft checks for project data BPMN cannot carry, such as dictionary links,
and lists what would be left out. You confirm before the file is written.
