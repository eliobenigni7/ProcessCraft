# Drawing on the canvas

The canvas follows the interaction model of bpmn-js, re-implemented in the desktop app. BPMN rules
decide what can connect to what and which kind of connection is created, so the diagram stays valid
while you sketch. An operation that would break the model is refused with a reason.

## Adding elements

- **The floating palette** adds an element. A click places it next to the selection and connects
  it; a drag puts it where you drop it.
- **The context pad** beside the selection appends the next task, gateway, event or annotation, or
  starts a connection, changes the type or deletes.
- **Single keys** add elements: `1` `2` `3` start, intermediate and end event, `T` task, `U` user
  task, `B` sub-process, `X` and `P` exclusive and parallel gateway, `N` annotation.
- **Dropping on a flow** inserts the element into that flow. **Dropping on an activity's border**
  attaches a boundary event, which then moves with the activity.

New elements are placed and their flows routed for you.

Every shape can be resized from the handles of the selection: four corners and four edge midpoints
(corners only for events, gateways and data references, which keep their proportions and keep their
labels centred below them). Dragged edges snap to neighbouring shapes and the grid. Hold `Shift` to
keep the proportions, `Alt` to resize around the centre, and `Ctrl` to resize without snapping. A
container never shrinks past the elements inside it. **Edit > Fit to content**, or a double-click on a
resize handle, sets the height of a task, sub-process or annotation to fit its text.

## Shortcuts

| Action | How |
|---|---|
| Connect, change type, delete | `C`, `R`, `Del`, or the context pad |
| Rename, edit annotation text | Double-click, `E` or `F2`; `Ctrl+Enter` finishes multi-line text |
| Pan and zoom | Drag empty canvas, middle mouse or `Space`+drag; `Ctrl`+wheel; `Ctrl` `+` `−` `0` |
| Fit | `Shift+1` fits the diagram, `Shift+2` zooms to the selection |
| Select | Click; `Shift`+click toggles; `Ctrl+A`; `Shift`+drag draws a box that adds to the selection: left to right takes what is fully inside, right to left whatever it touches; flows count when they lie fully inside |
| Move precisely | Arrow keys (1 px), `Shift`+arrows (10 px); hold `Shift` while dragging to keep one axis, `Alt` or `Ctrl` to drag without snapping |
| Clipboard | `Ctrl+C` `Ctrl+X` `Ctrl+V` `Ctrl+D`; pasting BPMN XML from another tool imports it |
| Find, space tool | `Ctrl+F`, `S` |
| Undo, redo | `Ctrl+Z`, `Ctrl+Y` |
| Save, open, new | `Ctrl+S`, `Ctrl+O`, `Ctrl+N` |
| Focus mode, bottom panel | `Ctrl+.`, `Ctrl+J` |

## Pools, lanes and sub-processes

Pools, lanes and sub-processes grow with their content. Lanes can be added, split, resized and
deleted. The space tool (`S`) opens room in a crowded diagram by pushing everything on one side of the
pointer.

## Tidying up

- Snapping guides and a 10 px grid help placement.
- Bendpoints and segments of a flow can be dragged, and either end of a flow can be reconnected.
- **Align** and **distribute** work on a selection.
- **Auto layout…** proposes a layout for the whole diagram and shows a preview before you accept it. Nodes stay in their lanes, branches and loops are untangled where possible, and the preview bar shows how many flow crossings and other warnings (overlapping labels or routes) remain, so you can decide whether to apply it.

Every edit, including an auto-layout, is one undo step.

## Properties

The **Properties** panel edits the selected element. Changes apply when you press Enter or leave the
field. Rarely used BPMN and XML details are under **Advanced BPMN**, collapsed by default.
