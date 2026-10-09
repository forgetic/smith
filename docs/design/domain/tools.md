# Tools

Provisional, 2026-10-05, revised 2026-10-09. What a smith session does to
its workspace, as a domain layer: read, list, search, write, edit and
shell. It is the child domain `smith-domain-tools`, under the session
(session.md). A run with no workspace has none of these tools; its LLM
acts through its host's tools and MCP servers. What is still open is
listed in section 8.

## 1. In one page

- **Typed calls, typed outcomes.** A tool call is a domain entity
  (`Read`, `Edit`, `Shell`, ...), decoded by the protocol layer from
  what the LLM wrote in its dialect's format, and so is its outcome
  (`Edited`, `Exited`, ...).
- **One path namespace.** Relative paths start at the working directory,
  other directories are reached by name, and a failure names the path as
  it was resolved (section 2).
- **Confined to the workspace,** its writes to the writable directories.
- **Read before write, match before edit.** A `write` needs the file's
  current version read; an `edit` needs each snippet to match once in
  the file as it is. Both are checked against the real file.
- **Bounded, told and visible.** Each tool's output has a limit, stated
  to the LLM with its unit and its maximum, and the LLM may choose its
  outputs' budgets within them. What a limit cuts is said, and a failure
  comes back with its output, not a fixed message.
- **Commands are bounded:** each in its own process group, with a
  deadline the LLM may choose within a stated maximum, and the
  configuration's environment, without credentials. Containment in a
  view of the workspace comes with skein's contained trees (section 5).

## 2. The workspace

- **Directories side by side.** A workspace is one or more directories
  (documents, data, notes, code), each mounted under the name the LLM
  calls it, so what refers to a sibling by path works. The first is the
  working directory: relative paths and commands start there, and the
  others are its siblings. Each may be writable or not; its host decides
  which (run.md, 3.2).
- **One path namespace,** the same for every tool, however many
  directories there are:
  - a relative path starts at the working directory;
  - `/name/...` is a path within the directory called `name`, and
    `list /` answers the directories' names from the session's
    authority, without io;
  - until contained trees give commands the same view, a host absolute
    path within a directory is an alias for the same place, so a path a
    command printed can be given to a tool. An absolute path at or
    beneath a directory's host path, compared by whole names, reads as
    that alias; any other reads as `/name/...`.
- **What the LLM is told depends on how many directories there are**
  (run.md, 3.3; protocol/llm.md, section 3). One directory hides its
  name: the LLM is told that it works in its working directory, and
  writes `src/lib.rs`, never the directory's name. With several, the
  working directory is named, and the others are reached as
  `/name/...`.
- **Failures name the resolved path.** A path not found, not a
  directory, outside the workspace or not writable comes back with the
  path it resolved to, normalised and bounded, so a name written as a
  prefix shows as the path it made.
- **The directory itself** is an empty path within it, and `.` at io for
  every operation: loads, stores, scans, listings, searches, and the
  working directory of a command or a check. `read`, `write` and `edit`
  of a directory itself answer *not a file* without asking io.
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
| `read` | read | a window of a file's lines, from the line asked for |
| `list` | read | the tree beneath a directory, to a depth, honouring ignore files |
| `search` | read | `rg` over the workspace |
| `write` | write | a file's whole content, created or replaced |
| `edit` | write | one or more replacements in one file, applied together or not at all |
| `shell` | write | a command, in its own process group, with a deadline |

The charter's families grant them: inspect (`read`, `list`, `search`),
modify (`write`, `edit`), shell.

- **`read { path, offset?, window? }`** answers the whole lines from line
  `offset` that fit its window of bytes: the profile's default, or the
  window asked for. The outcome says which lines it holds of how many,
  so the LLM pages by asking from the next. A line longer than the
  window is cut, and says so. Any read counts as reading the file at the
  version io loaded, however little of it the window shows.
- **`list { path, depth?, glob? }`** answers the entries beneath a
  directory, files and directories in path order, to `depth` levels or
  the whole tree, and only those whose names match `glob` when it is
  given. Entries the ignore files exclude are left out, as `rg` leaves
  them out, and git directories always. The outcome counts the entries
  beyond its cap.
- **`search { pattern, path?, glob? }`** answers the lines that match, in
  path order, within its caps on hits and bytes, and counts the rest.
- **`write { path, content }`** makes the file hold `content`, creating
  it if there is none.
- **`edit { path, edits }`** applies `edits` in order, each an `old`
  snippet, its `new` replacement and whether to replace `all`
  occurrences, each to the file as the edits before it left it. Each
  `old` must occur exactly once, or at least once with `all`. If any
  edit fails, none is applied, and the outcome names the first that
  failed and why. The file is stored once.
- **`shell { command, timeout_seconds?, output_bytes? }`** runs `command`
  with a deadline and a budget for its kept output (section 5).
- **Output budgets are the LLM's to choose.** A `read`'s window and a
  command's kept output are asked for per call, and clamped to the
  profile's ceilings; the outcome says when a request was clamped, and
  to what. A search's and a listing's caps are the profile's.
- **One typed call, whatever the dialect.** The protocol layer offers
  each dialect the edit format its models handle best, chosen by
  measurement, and decodes every format to the same `Edit`
  (protocol/llm.md, section 3). The domain knows one call.

## 4. Rules

- **Typed, both ways.** The protocol layer owns each tool's schema, the
  decoding, and the rendering of an outcome as the text the LLM reads;
  the domain decides the content, such as how much of a command's output
  to keep. A malformed call arrives as an `Invalid` call with a typed
  problem (a missing field, a wrong type, a call too large to take), and
  the domain decides what the LLM is told: for a call too large, how to
  make it fit.
- **The workspace is shared, knowledge is not.** What a session's LLM has
  read, and at which version, is that session's own state.
- **Read before write.** A `write` may replace a file only if its
  session's LLM has read the current version; creating a file needs no
  read, and a version the session wrote itself counts as read. The
  version is checked against the real file when writing, so a change
  made meanwhile by another session, or by anything else, is caught.
- **Match before edit.** An `edit` needs no earlier read. It loads the
  file as it is, needs each `old` to match there (section 3), and stores
  the result only if the file still has the version it loaded, so a
  change made between the two is caught. The version an edit stores
  counts as read.
- **Confined.** Paths resolve inside the workspace's directories, and
  writes land only in writable ones. Writes and edits follow no symbolic
  link on any part of their path, so a change lands in the directory its
  path names, and a path with a `.git` part, in any case, is refused: the
  host must not have to trust what a repository's own configuration
  became.
- **Limits are told.** Every limit a tool applies is stated to the LLM
  with its unit and its maximum: a read's window, what a write or an
  edit may carry, a command's deadline and kept output, a search's hits
  and bytes, a listing's entries. None is set here: each is derived
  from the deployment's declared quantities and checked at startup
  (protocol/limits.md), and the tools' descriptions state the effective
  values (protocol/llm.md, section 3).
- **One payload bound.** One declared tool payload bounds a `read`'s
  window, a `write`'s content and the call the LLM writes, so what can
  be read in one window can be written back in one call.
- **Bounded and visible.** Each tool's output has a size limit, and what
  a limit cut is said, with how much. A failure comes back with its
  output (a failing test's tail, an edit that matched nothing), not a
  fixed message.

## 5. Search and shell

- **The environment is the configuration's, whole.** The agent's
  configuration owns the commands' environment: what a build needs (a
  `PATH`, a home, a locale), and never a credential (protocol/agent.md,
  section 3). Commands and checks see exactly it; searches and listings
  see none. The domain carries no environment: no charter field and no
  call sets a variable.
- **Search** is `rg`, run as a process of its own: no configuration, the
  pattern and glob passed so that neither reads as an option, and an
  empty environment. A code graph served over MCP is to complement it,
  as a tool source of its own (run.md, 5.1).
- **Shell** runs a command with `sh`, in the working directory:
  - **its deadline** is the one the LLM asked for, or the profile's
    default, clamped to a tool call's longest deadline, a policy value of
    the profile, which the tool's description states (protocol/limits.md,
    section 2.3). The outcome says when a request was clamped. A session
    that runs out of time cancels its commands as it closes (session.md,
    section 6);
  - **its kept output** is the budget asked for, or the profile's
    default, within the ceiling: the head and the tail of what it wrote,
    the budget split between them in the proportion of the profile's
    head to its tail, and the count of bytes dropped between them.
- **A command has its own process group,** and a timeout signals the
  group, so what the command started ends with it, not only the shell.
  A process that leaves its group is beyond this until contained trees.
- **Containment is skein's contained trees, later.** A command's tree is
  then to see the workspace's view: it writes only the writable
  directories, and every git directory is read-only to it, since the
  host commits the checked tree, merges in progress included, and the
  LLM needs no git writes. Searches and listings see every directory
  read-only. Resource limits per tree come with them. Until then a
  command is not confined: `write` and `edit` keep section 4's rules,
  and a command has only the permissions of the agent's user
  (protocol/agent.md, section 3).

## 6. Below the domain

io's operations: file loads and atomic stores, directory scans and tree
listings, `rg` searches, and commands in their own process groups with
deadlines, and later contained process trees with proof that a tree is
empty once stopped, reached through `smith-protocol-machine`. That
component translates a place to an io path in one way for every
operation (section 2). The protocol layer: each tool's schema and each
dialect's edit format, decoding and rendering (`smith-protocol-llm`).

## 7. The world

The machine's faces for files and processes, and the programs they run
(git directories read-only, `rg`, `sh`). Its stories:

- **Paths:** one directory and several; `list /`; a host alias; the
  directory itself, for every operation; a directory's name written as a
  prefix, failing with the path it resolved to; symbolic links and
  `.git` parts on paths.
- **Changes:** files changed behind a session's back; a `write` of a file
  not read; edit lists that all match, one that fails so that none
  applies, edits that overlap or match twice, and an edit with no
  earlier read; a file changed between an edit's load and its store.
- **Reads and listings:** a file paged window by window, at the default
  window and at one asked for; a request past a ceiling, clamped and
  said; a tree cut at its cap, with ignored entries left out.
- **Commands:** commands that hang, fail and flood their output; a
  deadline asked past the maximum, clamped and said; a kept output asked
  for, split between head and tail in the profile's proportion; a
  command whose child outlives it, ended with its group at the deadline;
  the configured environment seen by commands and checks, and none by
  searches.

Its referee: nothing written by a tool outside the writable directories;
no write of a version not read; no edit stored over a version other than
the one it matched; every outcome within its stated limit; every call
answered once. Random edit lists drive the fuzzy world. These are domain
steps over the fake machine, within the suites' budgets; that a group's
grandchild dies at the deadline is skein's to prove on a real kernel.

## 8. Open questions

- **A code graph:** indexing a workspace for codebase-memory-mcp, served
  as an MCP tool source, and who keeps the index.
- **Workspaces beyond directories** (README.md, section 8).
- **Background processes:** a tool to start a command, read its output as
  it comes, write to it and stop it, after process groups.
- **Aliases that collide:** an absolute path that reads both as a host
  alias and as `/name/...` is read here as the alias; whether to refuse
  it as ambiguous instead. Aliases go once contained trees give the view
  fixed mounts, which the tools then accept.
- **A run's own environment:** a typed, bounded charter overlay, merged
  in one place, only once a host needs one.

## 9. From temper

temper's agent's tools domain, as it was, with `edit` taking several
replacements and needing a match rather than a read, `list` answering a
tree, output budgets the LLM chooses within ceilings, and one path
namespace; only the names of what it confines for (temper's worker)
become the host's.
