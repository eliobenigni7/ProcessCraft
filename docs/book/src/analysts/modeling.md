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

## Shortcuts

| Action | How |
|---|---|
| Connect, change type, delete | `C`, `R`, `Del`, or the context pad |
| Rename, edit annotation text | Double-click or `E`; `Ctrl+Enter` finishes multi-line text |
| Pan and zoom | Drag empty canvas, middle mouse or `Space`+drag; `Ctrl`+wheel; `Ctrl` `+` `−` `0` |
| Select | Click; `Shift`+click toggles; `Shift`+drag draws a box; `Ctrl+A` |
| Move precisely | Arrow keys (1 px), `Shift`+arrows (10 px); hold `Alt` to drag without snapping |
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
- **Auto layout…** proposes a layout for the whole diagram and shows a preview before you accept it.

Every edit, including an auto-layout, is one undo step.

## Properties

The **Properties** panel edits the selected element. Changes apply when you press Enter or leave the
field. Rarely used BPMN and XML details are under **Advanced BPMN**, collapsed by default.
