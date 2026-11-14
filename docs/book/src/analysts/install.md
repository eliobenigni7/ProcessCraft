# Install and first launch

## Get the app

ProcessCraft 0.2.1 runs on Windows 10 and 11, 64-bit. The
[release page](https://github.com/eliobenigni7/ProcessCraft/releases/tag/v0.2.1) has two files:

| File | What it is |
|---|---|
| `processcraft-0.2.1-windows-x64.msi` | Installer. Installs for your account, without an administrator prompt, adds a Start menu entry and opens `.processcraft` files. |
| `processcraft-0.2.1-windows-x64-portable.zip` | The same programs in a folder. Unzip and run `processcraft.exe`. Nothing is registered. |

The files are not signed, so Windows SmartScreen may say "Windows protected your PC". That means
nobody has vouched for the publisher, not that something is wrong with the file. Choose **More info**,
then **Run anyway**. The [download page](https://eliobenigni7.github.io/ProcessCraft/download.html)
lists the SHA-256 checksums so you can check the file first.

## The window

![The ProcessCraft window: a rail and project sidebar on the left, a procurement process on the canvas with the floating palette, and the Properties panel on the right](../../images/workbench-light.png)

- **The rail** on the far left has three places: **Library** (your models and folders), **Model**
  (the diagram you are editing) and **Dictionary** (roles, systems, documents and attributes).
- **The document header** above the canvas switches how you look at a model: **Model**, **Table**
  or **RACI**. Its **Export** button saves BPMN, draw.io, SVG, PNG or PDF.
- **The canvas** fills the middle. The floating palette adds elements; the zoom capsule sits in a
  corner beside the **Overlay** capsule.
- **The Properties panel** on the right edits the selected element. Rarely used BPMN details sit
  under **Advanced BPMN**.
- **The bottom panel** (`Ctrl+J`) has three tabs: **Problems**, **Comments** and **Revisions**.

## Start a project

**File → New project** (`Ctrl+N`) creates an empty project with one process. The empty canvas says
what to do next: press `1` to add a start event, or click **Add start event**.

To open an existing BPMN file from another tool, use **File → Open…** (`Ctrl+O`) or **Import BPMN…**.
See [Opening and exporting files](files.md).

Save with `Ctrl+S`. A project is a single `.processcraft` file. Keep it on a local disk: saving is
qualified on local NTFS drives only, not on network shares or synced folders.

Next: [the procurement tutorial](tutorial.md).
