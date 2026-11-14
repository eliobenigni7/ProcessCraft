# Tutorial: a procurement process

This tutorial builds a small purchase-request process, gives its activities a cost, checks it
against a few house rules, and exports a RACI matrix and a PDF. It takes about fifteen minutes.

At the end you will have this:

![A procurement process in two lanes: Request received, Review request, an Approved? gateway, Place order, and a Complete end event](../../images/procurement.png)

> The same process is built from a script in the repository's tests
> (`tests/fixtures/commands/procurement.json`), so you can compare your result with it. The pictures
> on this page are exports of that project.

## 1. Draw the happy path

1. Create a project with **File → New project** and press `1` to add a start event. Press `E` and
   name it **Request received**.
2. With the start event selected, click the task icon in the context pad next to it. ProcessCraft
   appends a task and connects it. Name it **Review request**, and press `R` to change it to a
   **User task**.
3. Append an **exclusive gateway** (`X`) and name it **Approved?**.
4. From the gateway, append a task named **Place order**, then an end event (`3`) named
   **Complete**.

You never place or route anything by hand: each appended element is positioned and connected for
you. Every step is one undo (`Ctrl+Z`).

## 2. Add the rejected branch

Select the gateway, press `C` (connect) and click the **Complete** end event. Select the new flow and
name it **Rejected**; name the other gateway branch **Approved**.

ProcessCraft only lets you draw connections BPMN allows. If a connection is not allowed, the hover
feedback turns red and the connection is refused with a reason.

## 3. Put the work in lanes

Create a pool around the process and split it into two lanes, **Request and review** and
**Procurement**. Drag **Place order** into the Procurement lane. Lanes grow with their content.

To make the lanes mean something, open the **Dictionary** on the rail, add two roles (**Requester**
and **Procurement**), then select each lane and link it to its role in the **Responsibility & systems** section of the
Properties panel. The lane
role is the **R** (responsible) of every activity in that lane.

## 4. Give activities a cost

1. In the **Dictionary**, open the **Attributes** tab and click **Add starter attributes**. This
   creates Duration, Cost, SLA, Risk, Control and Process owner. Select **Cost** and mark it as
   required for activities. You can delete the attributes you do not need, or create your own.
2. Select **Review request**. The Properties panel now has an **Attributes** section: enter a cost.
   Do the same for **Place order**.
3. Switch the document header to **Table** to see every activity in one grid. **Attribute columns…**
   adds a column per attribute, and the cells are editable.

## 5. See the costs on the diagram

Open the **Overlay** capsule next to the zoom controls, pick **Cost** and choose **Heatmap**: each
activity is coloured in one of five steps from the cheapest to the most expensive, with a legend.
**Badge** shows the value on each activity instead.

![An onboarding process coloured by cost in five steps, with a legend](../../images/cost-heatmap.png)

The overlay is a way of looking at the model. It is not saved and does not change the project.

## 6. Check the house rules

Open the bottom panel (`Ctrl+J`), go to **Problems** and click **Conventions…**. Each rule can be
**Off**, **Warning** or **Error**. Turn on, for example, **Labels on gateway flows**, **No implicit
split or join** and **Required attributes have values**, then **Apply**.

For the process above, the Problems tab now reports:

- *Element has several incoming flows; use a gateway to join.* The end event is reached by both
  branches. Add an exclusive gateway before **Complete** to join them, or accept the warning.
- *Required attribute "Cost" has no value.* For any activity you skipped in step 4.

Click a finding to select the element it is about. The profile is saved in the project, so everyone
who opens the file sees the same rules.

## 7. Fill in the RACI matrix

Switch the document header to **RACI**. Rows are activities and columns are roles. The **R** comes
from the lane's role. Click **A**, **C** or **I** in a cell to toggle that letter for the role.

| Activity | Finance manager | Procurement | Requester | Legal |
|---|:-:|:-:|:-:|:-:|
| Review request | C | A | R | I |
| Place order | I | R, A | C | |

**Export CSV** saves the matrix for a spreadsheet.

## 8. Share it

Save the project (`Ctrl+S`). Then use **Export** in the document header:

- **Export PDF…** or **Export PNG…** for a document or a slide. While an overlay is on, tick
  **Include overlay and legend** to export the heatmap too.
- **Export BPMN…** for Camunda Modeler, bpmn.io and other BPMN tools. Attribute values travel in a
  ProcessCraft extension; see [Opening and exporting files](files.md).

## Where to go next

- [Drawing on the canvas](modeling.md): every shortcut.
- [Attributes, heatmaps and the activity table](attributes.md): attribute types and rules.
- [Conventions and RACI](conventions.md): every rule and what it checks.

> **Verification note.** The steps of this tutorial follow the desktop app's controls as built in
> 0.2.0. The same workflow has been run end to end through the command-line tool; a manual run in the
> desktop app is still one of the open checks on the
> [status page](https://eliobenigni7.github.io/ProcessCraft/status.html). If a step does not match
> what you see, please [open an issue](https://github.com/eliobenigni7/ProcessCraft/issues).
