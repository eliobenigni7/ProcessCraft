# Conventions and RACI

## Modeling conventions

A conventions profile turns your house rules into findings in the **Problems** tab. It is stored in
the project, so everyone who opens the file sees the same rules.

Open it from the bottom panel (`Ctrl+J`): **Problems → Conventions…**. Each rule is **Off**,
**Warning** or **Error**; **Apply** saves the profile as one undo step.

| Rule | Reports |
|---|---|
| Label required | elements of the chosen kinds without a name |
| Activity label is verb + object | activity names that do not start with a verb. The check uses an English word list you can replace, so it misses some verbs. |
| Labels on gateway flows | unnamed flows leaving a gateway that splits |
| No implicit split or join | an activity or event with several outgoing or incoming flows where a gateway belongs |
| Single start and end event per scope | a process or sub-process with more than one start or end event |
| Maximum shapes per diagram | diagrams over the limit you set |
| Required attributes have values | elements where a required attribute is empty |
| Dictionary terms | lanes without a role, and free text where a dictionary entry belongs |

Click a finding to select its element. The findings are advisory checks of this application. They are
not a statement of BPMN conformance, and a model with no findings is not necessarily complete or
correct.

The same profile and findings are available outside the app: `processcraft-cli validate` exits with
an error code when a finding reaches the threshold you choose, so a team can check models in CI. See
[The command-line tool](../developers/cli.md).

## RACI matrix

Switch the document header to **RACI**. Rows are the activities of the process, columns are roles.

- **R** (responsible) comes from the role linked to the activity's lane. To change it, link the lane
  to another role or move the activity to another lane.
- **A** (accountable), **C** (consulted) and **I** (informed) are stored on the activity. Click a
  letter in a cell to toggle it.

| Activity | Finance manager | Procurement | Requester | Legal |
|---|:-:|:-:|:-:|:-:|
| Review request | C | A | R | I |
| Place order | I | R, A | C | |

**Export CSV** writes the matrix as UTF-8 with a byte order mark, so spreadsheet programs open it with
the right characters. The matrix shows what the model records; it does not check it against a policy.
