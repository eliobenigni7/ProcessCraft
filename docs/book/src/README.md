# ProcessCraft documentation

ProcessCraft is a desktop workbench for BPMN processes that runs on your computer: draw a process,
give its steps attributes such as cost and owner, check it against your house rules, and exchange it
as BPMN, PDF or an image. A command-line tool and a local MCP server run the same commands as the
canvas.

[Back to the ProcessCraft site](https://eliobenigni7.github.io/ProcessCraft/)

These pages describe version 0.2.2, which is pre-alpha and runs on Windows 10 and 11.
[The status page](https://eliobenigni7.github.io/ProcessCraft/status.html) says what has been
verified and what has not.

## Who are you?

<div class="pc-paths">
<a href="analysts/install.html"><b>Process analyst</b><span>Install the app, follow the
procurement tutorial, then learn the canvas, attributes, conventions and exports.</span></a>
<a href="developers/cli.html"><b>Developer</b><span>Script ProcessCraft from the command line,
write JSON batches, read the file format, or build from source.</span></a>
<a href="agents/index.html"><b>AI agent</b><span>Drive ProcessCraft through MCP. Every page is
also plain Markdown, and <code>llms.txt</code> indexes them.</span></a>
</div>

## Three ideas that hold everywhere

- **One file per project.** A `.processcraft` file holds every document, diagram, dictionary entry,
  attribute value, comment and checkpoint of a project. Nothing is stored in an account or online.
- **One way to change a model.** The canvas, the command-line tool and the MCP server all submit the
  same typed commands. A group of commands is applied completely or not at all, and is one undo step.
- **Nothing is lost silently.** BPMN content that ProcessCraft cannot edit is kept byte for byte or
  refused with a reason.

Found a mistake on a page? Every page has an edit link at the top, and
[issues](https://github.com/eliobenigni7/ProcessCraft/issues) are welcome.
