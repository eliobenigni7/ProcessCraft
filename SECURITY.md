# Security policy

## Supported versions

ProcessCraft is pre-alpha. Security fixes go into the next release; only the latest release on the
[Releases](https://github.com/eliobenigni7/ProcessCraft/releases) page is supported.

## Reporting a vulnerability

Please report it privately, not in a public issue:

- **Preferred:** [open a private security advisory](https://github.com/eliobenigni7/ProcessCraft/security/advisories/new)
  on GitHub.
- **Or** write to eliobenigni7@gmail.com with "ProcessCraft security" in the subject.

Include the version, what an attacker could do, and the smallest file or steps that reproduce it. A
crafted `.processcraft` or BPMN file is the most useful reproduction. Please do not attach real
business processes.

ProcessCraft has one maintainer, so response times are best effort. You can expect an
acknowledgement within a week and an assessment after that. Once a fix is released, the advisory
is published and you are credited, unless you prefer not to be.

## What is in scope

ProcessCraft opens files that may come from other people, and it can be driven by AI agents. Issues
in these areas matter most:

- **File parsing:** BPMN XML import (`processcraft-io/src/bpmn.rs`) and the `.processcraft` package
  reader, for example memory or CPU exhaustion past the documented limits, entity expansion, or reading
  files outside what was opened.
- **File access from automation:** the MCP server writing or reading outside its `--read-root` and
  `--write-root` folders, or outside the folders authorized in the desktop app.
- **Attachment:** the Windows named pipe that lets `processcraft-cli mcp --attach` drive the open
  desktop session, for example acting without the token or without the user switching attachment on.
- **Saving and recovery:** losing or corrupting a project in a way the atomic save and recovery
  should prevent.
- **Release artifacts:** the MSI and portable ZIP. They are not yet code-signed; that is a known
  limit, not a vulnerability.

Out of scope: problems that need an attacker who already controls your Windows account, and
findings in third-party dependencies with no effect on ProcessCraft (please report those upstream).
