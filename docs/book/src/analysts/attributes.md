# Attributes, heatmaps and the activity table

Attributes are your own fields on BPMN elements: cost, duration, owner, risk, anything your analysis
needs. They are defined once per project and stored in the `.processcraft` file.

## Defining attributes

Open the **Dictionary** on the rail and its **Attributes** tab. **Add starter attributes** creates
the ones that are missing from this set: Duration, Cost, SLA, Risk, Control and Process owner. Nothing
is created unless you ask for it.

Each definition has:

- a **name** and an optional description;
- a **type**:

  | Type | Holds |
  |---|---|
  | Text | free text |
  | Number | a decimal number, with an optional unit |
  | Currency | an amount with a currency |
  | Duration | a length of time |
  | Enum | one option from a list you define |
  | Date | a calendar date |
  | Dictionary reference | roles, systems or documents from the dictionary |
  | URL | a link |

- **applies to**: the element kinds it belongs to, for example activities, events, lanes or
  sequence flows;
- **required**: whether the conventions check reports elements where it is empty.

Numbers are stored as exact decimals, so totals and buckets do not drift by rounding.

Deleting a definition, or changing its type, asks what to do with the values already entered: clear
them, move them to another attribute of the same type, or cancel.

## Entering values

- **One element**: select it; the Properties panel has an **Attributes** section with an editor for
  each attribute that applies. Invalid input stays in the field with a message and is not saved.
- **Many activities**: switch the document header to **Table**. **Attribute columns…** chooses which
  attributes appear as columns. Cells are editable; with several rows selected, one value is applied
  to all of them in one undo step.

## Heatmaps and badges

The **Overlay** capsule next to the zoom controls colours the diagram by one attribute:

- **Heatmap** for Number, Currency and Duration attributes uses five equal steps between the lowest
  and the highest value in the document; for an Enum it uses one colour per option, up to eight.
- **Badge** writes the value on each activity.

![The same process with heatmap colours and with value badges](../../images/cost-badges.png)

The overlay is screen-only: it is not saved, does not change the project and does not mark it as
modified. To put it in an image or a PDF, tick **Include overlay and legend** in the export menu while
the overlay is on.

## In BPMN files

Attribute values are exported inside each element's `extensionElements`, in the namespace
`urn:processcraft:attributes:1`, and read back when the file is imported into ProcessCraft. Dictionary
references cannot be carried this way: the exporter tells you and asks you to confirm. Whether other
BPMN tools keep these values when they save the file has not been tested.
