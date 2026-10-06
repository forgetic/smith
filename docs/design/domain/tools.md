# Tools

Provisional, 2026-10-05. What a smith session does to its workspace, as
a domain layer: read, list, search, write, edit and shell. It is the
child domain `smith-domain-tools`, under the session (session.md). A run
with no workspace has none of these tools; its LLM acts through its
host's tools and MCP servers. What is still open is listed in section 8.

## 1. In one page

- **Typed calls, typed outcomes.** A tool call is a domain entity
  (`Read`, `Edit`, `Shell`, ...), decoded by the protocol layer from the
  JSON the LLM wrote, and so is its outcome (`Edited`, `Exited`, ...).
- **Confined to the workspace,** its writes to the writable directories.
- **Read before write,** checked against the real file.
- **Bounded and visible.** Each tool's output has a limit set here, and a
  failure comes back with its output, not a fixed message.
- **Processes are contained,** with deadlines and an environment without
  credentials.

## 2. The workspace

- **Directories side by side.** A workspace is one or more directories
  (documents, data, notes, code), each mounted under the name the LLM
  calls it, so what refers to a sibling by path works. Each may be
  writable or not; its host decides which (run.md, 3.2).
- **Repositories are directories with git.** A directory that is a git
  working tree gets git's protections: its git directory is never
  written (section 4), and it may start from a merge in progress.
- **Files are world state,** reached through io and shared by every
  session of the run. Write authority is copied into a session when it
  opens.
- **A merge in progress** is a repository like any other, whose
  conflicted files hold their markers until the LLM resolves them
  (run.md, 8.3).

## 3. The tools

| Tool | Effect | What it does |
|---|---|---|
| `read` | read | a file's lines, in a window |
| `list` | read | a directory's entries |
| `search` | read | `rg` over the workspace |
| `write` | write | a file's whole content, created or replaced |
| `edit` | write | a snippet replaced in a file |
| `shell` | write | a command, in a contained process tree |

The charter's families grant them: inspect (`read`, `list`, `search`),
modify (`write`, `edit`), shell.

## 4. Rules

- **Typed, both ways.** The protocol layer owns each tool's schema, the
  decoding, and the rendering of an outcome as the text the LLM reads;
  the domain decides the content, such as how much of a command's output
  to keep. A malformed call arrives as an `Invalid` call with a typed
  problem (a missing field, a wrong type), and the domain decides what
  the LLM is told.
- **The workspace is shared, knowledge is not.** What a session's LLM has
  read, and at which version, is that session's own state.
- **Read before write.** A session may change a file only if its LLM has
  read the current version; creating a file needs no read. The version is
  checked against the real file when writing, so a change made meanwhile
  by another session, or by anything else, is caught.
- **Confined.** Paths resolve inside the workspace's directories, and
  writes land only in writable ones. Writes and edits follow no symbolic
  link on any part of their path, so a change lands in the directory its
  path names, and a path with a `.git` part, in any case, is refused: the
  host must not have to trust what a repository's own configuration
  became.
- **Bounded and visible.** Each tool's output has a size limit set here,
  and a failure comes back with its output (a failing test's tail, an
  edit that matched nothing), not a fixed message.

## 5. Search and shell

- **Search** is `rg`, run as a contained process: no configuration, the
  pattern and glob passed so that neither reads as an option, an empty
  environment, and read-only directories. A code graph served over MCP
  is to complement it, as a tool source of its own (run.md, 5.1).
- **Shell** runs in a contained process tree (io), with a deadline, the
  head and tail of its output captured, and an environment without
  credentials. The tree's view of the workspace is the confinement: a
  command writes only the writable directories, and every git directory
  is read-only to it, since the host commits the checked tree, merges in
  progress included, and the LLM needs no git writes.

## 6. Below the domain

io's operations: file loads and atomic stores, directory scans, `rg`
searches, contained process trees with deadlines and proof that a tree
is empty once stopped. The protocol layer: each tool's schema, decoding
and rendering (`smith-protocol`).

## 7. The world

The machine's faces for files and processes, and the programs they run
(git directories read-only, `rg`, `sh`): files changed behind a
session's back, edits that match nothing or twice, symbolic links and
`.git` parts on paths, commands that hang, fail and flood their output.
Its referee: nothing written outside the writable directories; no write
of a version not read; every call answered once.

## 8. Open questions

- **The environment commands run with:** empty today, so no `PATH`; what
  a command needs to build and test, without credentials, and whether a
  charter names it.
- **A code graph:** indexing a workspace for codebase-memory-mcp, served
  as an MCP tool source, and who keeps the index.
- **Workspaces beyond directories** (README.md, section 8).

## 9. From temper

temper's agent's tools domain, unchanged in what it does; only the names
of what it confines for (temper's worker) become the host's.
