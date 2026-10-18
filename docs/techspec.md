# Technical Specification: shimbox

Status: v1.0. Sections 2 to 6 describe what the shipped code does; section 7 is
how it was built and section 8 what is still open. Draft 1 was reviewed and its
findings folded in.

## 1. Project Overview

A lightweight executable-shimming tool for Windows: it generates tiny proxy
executables that live in a directory on `PATH` and forward invocations to the
real binaries. Two modules:

1. **`shim-runner`** (crates `runner`, `runner-cli`, `runner-gui`): a minimal
   Rust executable, shipped in two variants (console / windows subsystem) built
   from one library. Reads its companion config file and launches the target with
   the caller's arguments.
2. **`shimbox`** (crate `gui`): a Tauri desktop app that generates individual
   shims and batch-generates them for a whole directory.

v1 scope is **generation only**: the app creates shims, it does not list,
inspect, edit or delete existing ones.

---

## 2. Architecture & Components

```text
[ User / Terminal ]
        |
        v
[ <alias>.exe ]  (a copy of one shim-runner variant)
        | reads
        v
[ <alias>.shim ] (companion config, same directory)
        | launches, inherits stdio, waits, propagates exit code
        v
[ Real Binary: C:\Tools\ffmpeg\bin\ffmpeg.exe ]
```

### 2.1 Two runner variants (decided at generation time)

| Variant | PE subsystem | Behaviour |
| :--- | :--- | :--- |
| **CLI** | `IMAGE_SUBSYSTEM_WINDOWS_CUI` | Inherits the caller's console: stdin/stdout/stderr and the wait behaviour come from the OS for free. Child is spawned with inherited handles. |
| **GUI** | `IMAGE_SUBSYSTEM_WINDOWS_GUI` | No console window, shell does not block. Child is spawned with `CREATE_NO_WINDOW`. |

The variant is encoded in the bytes written to `<alias>.exe`, and those bytes are
two crates over one library rather than one crate built twice. **It is not a
config field** — nothing reads it while running, and the GUI has no reader for
it. The runner type dropdown in the UI (5.4) selects which embedded binary
to write.

Consequences worth keeping in mind:
- The runner must **not** pipe stdio by hand. Inheritance is what makes
  interactive programs, progress bars and TTY detection work.
- The runner must **not** install a console Ctrl+C handler. Both shim and child
  are in the same console group, so a hand-rolled handler swallows the second
  Ctrl+C that target programs often rely on.
- Errors in the GUI variant cannot be reported on stderr (there is none) — see
  FR-1.6.
- Which variant a given target needs is decided by the GUI at generation time,
  never by the runner (5.4).

### 2.2 Components

* **`shim-runner` (`runner` is the shared library; `runner-cli` and `runner-gui`
  are the two variants, each a `main` of a few lines over it)**
  * Windows-only in v1.
  * No third-party crates in the runtime path: the config parser is hand-written
    (section 3), and the Win32 calls it needs (`GetCommandLineW`, `MessageBoxW`,
    and `CredReadW`/`CredFree` for a `cred:` reference) are declared as
    `extern "system"` rather than pulled in via the `windows` crate.
  * Each variant's `build.rs` writes the `VERSIONINFO` resource its shims carry —
    a generated `<alias>.exe` is a byte copy of the runner, so that resource is
    the only place the tool can explain one of its own files to somebody who
    opens Properties without having read the README. The two scripts include one
    file, `crates\runner\winres.rs`, and name the variant in the two lines the
    resource differs by. `tauri-winres` is a build-dependency for it: the crate
    contributes the ~1 KB of metadata and nothing to the code.
  * Size target: **<= 400 KB** per variant with `opt-level = "z"`, `lto = "fat"`,
    `codegen-units = 1`, `panic = "abort"`, `strip`. Draft 1 targeted 150 KB,
    which is not reachable while linking `std`; going below it requires
    `-Z build-std` or dropping `std`.
  * Added latency target: **<= 3 ms** over a direct invocation. Process creation
    alone costs roughly 0.5–1.5 ms, so anything under ~3 ms is unmeasurable
    against the noise.

* **`shimbox` (crate / Tauri app `gui`)**
  * Backend: Rust (Tauri v2). Frontend: Vanilla JS with no build step and nothing
    it needs in order to run (section 8) — its one npm file is Prettier, a
    dev-time formatter that checks how the code is wrapped and is never read by the
    webview; the UI is two tabs and one table, so no
    build-chain-heavy stack is needed for its sake.
  * Embeds **both** runner variants via `include_bytes!` and writes the selected
    one.
  * Dev loop: the embedded bytes go stale against a freshly built runner, so
    `build.rs` copies the staged `bin\runner-*.exe` into `OUT_DIR` and watches
    them — a runner rebuilt without the GUI rebuilding is a build error rather
    than a silent embedding of the old one. `scripts\build.cmd` keeps the order;
    building the GUI alone fails until `bin\` holds a runner.
  * Its own `VERSIONINFO` is written by `tauri-build` out of `tauri.conf.json`,
    which is the only route that does not fight the linker over who owns the
    resource. That leaves the version number written twice — once in the
    workspace, once in the config — and `build.rs` compares the two and fails the
    build on a difference, so the duplication is guarded rather than trusted.

### 2.3 Repository layout

The root stays small: build tooling and generated artifacts live below it.

```text
shimbox/
  Cargo.toml            workspace manifest
  Cargo.lock
  README.md
  LICENSE
  .gitignore            /target, /bin, tauri's generated schemas
  .github/workflows/
    ci.yml              the same three scripts, on a Windows runner
  bin/                  build output, ignored
                        (runner-cli.exe, runner-gui.exe, shimbox.exe, one zip)
  docs/
    techspec.md
    images/             the screenshots the README shows
  scripts/
    common.ps1          shared helpers, dot-sourced only
    build.ps1           master: tests + both variants + size report, and links the
                        manager into bin unless its window is open
    smoke.ps1           end-to-end runner checks against real binaries
    package.ps1         the release archive, from what bin/ already holds
    build.cmd           thin wrappers, no logic
    test.cmd
    smoke.cmd
    package.cmd
  crates/
    runner/             shim-runner: the machinery both variants share, and the
                        version resource they both describe
    runner-cli/         the console-subsystem executable
    runner-gui/         the windows-subsystem executable
  apps/
    gui/
      assets/           icon source
      package.json      the formatter's manifest; a devDependency and no runtime one
      prettier.config.mjs  how the UI is wrapped, measured off the UI itself
      src-tauri/        Rust backend, embeds ui/ and both runners
      ui/               frontend: no build step, nothing it needs in order to run
        index.html
        main.js         the two tabs, the status line, what is built when
        i18n.js         language resolution and the keys left on the nodes
        theme.js        the theme, and what Windows is told about it
        titlebar.js     the strip that replaced the native caption
        form.js         widgets both screens build the same way
        environment.js  the child-environment rows of mode 1
        secrets.js      the store a `cred` row points at
        settings.js     the card holding the theme and language choices
        about.js        the card naming the build
        update.js       the once-a-day question about a newer release
        app.css
        locales/        one file per language
        screens/        one file per mode
```

Rule of thumb: nothing new is added to the root directory without a reason;
scratch scripts go to `scripts/`, notes to `docs/`.

---

## 3. Configuration Format (`<alias>.shim`)

Every generated `<alias>.exe` requires an adjacent `<alias>.shim` with the same
base name.

### 3.1 Why not JSON

Draft 1 left the format as "JSON or INI". JSON was incompatible with the
zero-dependency rule and with the size target, and it would need a parser in two
places. The format is therefore a flat `key=value` file, ~40 lines of
hand-written parsing in the runner. The GUI **writes** this format, and reads back
one key of it and nothing else — the `runner` line 5.7 rests on for the half of a
pair whose `.exe` is gone — so no second parser exists.

### 3.2 Syntax

```ini
# shimbox config
runner=cli@1.0.0
path=C:\Tools\ffmpeg\bin\ffmpeg.exe
arg=-hide_banner
workdir=C:\Tools\ffmpeg
env=FFMPEG_DEBUG=1
envref=GITHUB_TOKEN=cred:shimbox/github
```

* UTF-8, CRLF or LF accepted, and a leading byte-order mark tolerated rather than
  treated as part of the first key. Lines are split on the first `=`.
* The key is what precedes that `=`, trimmed and read without case, so
  ` Path =C:\Tools\x` is a `path`. It is the value that is taken literally.
* The value is the remainder of the line **verbatim**, with only a trailing
  `\r` removed. Values are not quoted, escaped or trimmed — backslashes and
  spaces are literal, so no escaping rules exist to get wrong.
* `#` at the start of a line (after optional whitespace) is a comment.
* Unknown keys are ignored.

### 3.3 Keys

| Key | Required | Meaning |
| :--- | :--- | :--- |
| `runner` | written by the GUI, ignored by the runner | `<variant>@<version>` of the bytes copied in as the `.exe` half. What 5.7 compares is those bytes themselves, so this line is the word the pair is left with once its runner is gone, and a note for whoever reads the file by hand. The runner never looks at it: it already *is* those bytes. |
| `path` | yes | Absolute path to the target executable. |
| `arg` | no, repeatable | Argument inserted before the caller's arguments, **exactly as written** — the GUI does not add quoting, so `arg=-o "out file.txt"` reaches the child as `-o "out file.txt"`. |
| `workdir` | no | Working directory for the child. |
| `env` | no, repeatable | `NAME=value` set for the child, the value exactly as written (3.5). |
| `envref` | no, repeatable | `NAME=reference` whose value is read at launch, so a secret never sits in the file (3.5). |

`path` is stored absolute. Relative-to-config resolution is deferred
(section 8), as is anything non-`.exe` in `path`.

### 3.4 `workdir` default (changed from draft 1)

When `workdir` is absent, the child's working directory is **the target's own
directory**, not the caller's current directory.

Draft 1 defaulted to the caller's cwd, which silently breaks portable apps that
load DLLs and data files relative to their working directory. The caller's cwd
is still what the shim itself sees, so nothing about stdin/stdout or `.` in the
child's *arguments* changes — only its `GetCurrentDirectory`. Explicit
`workdir`, including a path pointing back at the caller's cwd, overrides this.

### 3.5 The child's environment

`env=NAME=value` is the plain case: the pair is added to the environment the
child inherits, so a name the caller already set is overwritten and an empty
value sets it empty rather than removing it. Values are verbatim, as everywhere
else in the file, which means a value may contain `=` — only the first one
separates the name from it.

`envref=NAME=reference` says where the value lives instead of holding it. The
reason is the file's own address: a config sits in a folder on `PATH`, in the
open, next to the runners.

| Reference | Reads |
| :--- | :--- |
| `file:<path>` | The whole file, without its trailing newlines. |
| `cred:<target>` | The blob of a generic credential, i.e. one entry of the Windows Credential Manager. |

* Credentials are named under a `shimbox/` prefix and the GUI only writes and
  offers names under it, so a generated config cannot point at another program's
  stored secret. The runner itself does not check the prefix — it reads the name
  it is given, which is why a hand-written `cred:` line can still ask for any
  credential of this user. A blob is read as UTF-8, which is what
  shimbox writes, and as UTF-16 when it holds a NUL, which is what `cmdkey`
  writes.
* One name serves any number of shims, which is the point of referring by name
  rather than by value: a token is created once and each shim calls it whatever
  its target expects.
* A reference is resolved when the shim runs, and an unresolvable one is a
  failure rather than a child started without a value it expects (FR-1.8).
* What this buys is that the value is not **in** the file. It is not secrecy
  from the account: a generic credential is a DPAPI blob the user's own session
  can decrypt, so anything running as that user can read it the same way. The
  system caps a blob at 2560 bytes, which is a token's size and not a data
  file's — for anything larger, `file:` is the reference to use.

### 3.6 The same extension as Scoop

`.shim` is not an unclaimed name: Scoop's `scoop-shim` uses it for the same
shape — a text config sharing the base name of a stub executable copied into a
folder on `PATH`. Checked against the implementation that is in Scoop today
(`ScoopInstaller/Shim`, `rust/shim.rs`) rather than assumed, and the two formats
are not interchangeable in either direction:

| | shimbox | scoop-shim |
| :--- | :--- | :--- |
| separator | the first `=` | a literal `" = "`, spaces and all |
| value | verbatim | trimmed left, one pair of surrounding quotes removed, `%VAR%` and `%~dp0` expanded |
| arguments | `arg=`, repeatable, one argument per line | `args =`, one line split by `CommandLineToArgvW` |
| comments | `#` | `#`, `;`, `//` |
| a repeated `path` | last wins | first wins |
| unknown keys | ignored | become variables for the child |

Both sides trim the key and read it without case, which is the one place they
agree — and it is enough to make a mix-up loud rather than wrong:

* The runner matches a Scoop config's `path` key but keeps its value as written,
  leading space and quotes included, and says so:
  `cannot start C:\Shims "C:\Tools\x.exe": … (os error 267)`, exit **103**. The
  quotes in the message are their own explanation. This one was run against a
  real Scoop config rather than reasoned about.
* scoop-shim reads a shimbox config as no lines at all, because `path=C:\…`
  holds no `" = "` to find, and reports
  `Shim: 'path' not found in shim file '…'` — their string, from the same file.

The extension is kept regardless. Since neither tool can run the other's file,
nothing is won by renaming it: `ffmpeg.shim` beside `ffmpeg.exe` already says
what it is to anyone who has met Scoop, and a folder holding both kinds is
handled — mode 2 neither overwrites an existing `.shim` nor offers a file that
already has one beside it (5.3), mode 1 refuses to write over one, and mode 3
refuses to write over a pair whose `.exe` does not name shimbox in its own
resources (5.7).

---

## 4. Functional Requirements: `shim-runner`

* **FR-1.1 Self-identification**: the runner derives its own stem and directory
  from `std::env::current_exe()`. Hard links are not used for the generated
  shims in v1 (section 8); with hard links this must switch to `argv[0]`,
  because `current_exe()` may return any of the link names and therefore the
  wrong config file.
* **FR-1.2 Config resolution**: reads `<stem>.shim` from that same directory.
* **FR-1.3 Transparent argument forwarding**: does **not** re-serialize
  `std::env::args()`. Rust parses the Windows command line with MSVC rules and
  re-quotes on the way out, and arguments containing backslash-before-quote
  sequences do not survive that round trip. Instead the runner takes the raw
  command line (`GetCommandLineW`), strips the `argv[0]` portion, and passes the
  remainder to the child via `Command::raw_arg`. The resulting child command
  line is:

  ```text
  <path> <each arg from config, in file order> <raw caller tail, byte-for-byte>
  ```
* **FR-1.4 Stdio**: inherited, not piped (section 2.1). No Ctrl+C handler.
* **FR-1.5 Exit code propagation**: waits for the child and exits with the
  child's code.

  | Condition | Exit code | Note |
  | :--- | :--- | :--- |
  | the config cannot be read — missing, locked, or not UTF-8 | 101 | The name the runner tried is in the message, which is also how a missing config and an unreadable one are told apart by eye. |
  | the config reads but names no `path`, or the runner cannot derive its own name | 102 | |
  | spawn failed | 103 | Last Win32 error is included in the message. |
  | an `envref` that names nothing readable | 104 | The child is not started: a target that expects a value it never gets fails later and less legibly. |
  | otherwise | child's code | Cannot be distinguished from a child that itself returned 101–103; accepted. A child that reported no code at all, because it was killed, leaves 103 in its place. |

* **FR-1.6 Error reporting**: on any runner-side failure, the CLI variant writes
  one line to stderr naming what failed — the config, the target or the
  reference — and the reason; the GUI variant shows a `MessageBoxW` with the same
  text, since it has no console.
* **FR-1.7 No fallback discovery**: if the config is missing or `path` is not
  found, the runner fails as above. It does not search `PATH`, does not guess,
  and does not prompt.
* **FR-1.8 Child environment**: the `env` pairs are applied in the order the
  file holds them, and the `envref` ones after them, each resolved at launch
  (3.5), so a reference wins over a literal of the same name and the last of two
  like it wins. Nothing is removed from the inherited environment: the child
  sees the caller's variables plus these.

---

## 5. Functional Requirements: `shimbox`

Two tabs, no other navigation. Both screens are built once and a tab only shows
one of them, so a half-written form and a scan someone is still ticking survive a
look at the other mode — nothing is re-asked and nothing is cleared by switching.

The shell reads as a Windows application rather than a web page: the window sits
on a Mica backdrop and the interface is semi-opaque cards over it. No colour of
the palette is picked by hand — each one is a token declared once per theme, and
the accent is `Highlight`, the colour the user chose in the system. The one value
written outside the tokens is the caption's close red, which Windows paints the
same way under either theme. Which of the two themes is
drawn is a decision made in code rather than a stylesheet branch: the choice is
`System`, `Light` or `Dark`, `System` asks `prefers-color-scheme` and keeps
asking while the desktop is changed under it, and the answer is written onto the
document as `data-theme`, so one block of dark tokens serves both the system's own
value and an override. Windows is told the same answer, because the backdrop it
composites, the scrollbars it draws and a native dropdown are not part of the page
and would otherwise stay on the system's. The choice is stored in
`$APPCONFIG/.settings.json`; a value the app does not offer, and a file that cannot
be read, both mean the same thing as no file at all. The language is the same kind
of choice in the same card, and it is offered the same way: follow Windows, which
asks `navigator.languages`, or one of the languages the app ships. Each is named in
its own tongue — `English`, `Русский` — because a language has to be recognisable
before the interface speaks it. It applies the way a theme does, by rewriting the
text that stands on the screen rather than by building the window again: every
string the window shows is left on its node by key, in each of the places one can
stand — visible text, a placeholder, an accessible name, a tooltip — so a change is
one pass over the document and a half-written form keeps what it holds. A sentence
that counts something keeps its numbers beside its key, so the next language
rebuilds that sentence instead of repeating the old one. A result card and the
About card are written when they open, so they are read in the language that is in
effect at that moment. The row
is only there while there is a choice to make, so a single-language build shows the
card with its one field again. A field
is one block of caption, control and hint, and the blocks reflow from as many
columns as the width allows — three at the default window — down to one with it.
A result is reported in a dialog over the form rather than below it: the form is
often taller than the window, and whether the
write worked is the last thing in it that should need a scroll to be found. The
dialog is a card over a frame that dims the whole window, so a click anywhere
outside the card ends the result; `Esc` and its own Close action do the same, and
the body is the part that gives when a batch summary is longer than the window. A
result that wrote something also offers the two ways to act on the folder it
wrote into: open it, and open the system editor that puts it on `PATH` (6).
Each screen is one card that takes the height the window leaves rather than pushing
the window past it, and one region inside it gives: the fields in mode 1, where
every variable the target needs adds a line, and the scan in mode 2, where a folder
can hold forty programs — scrolling under a header that stays put, and dropping
Version and Size below the width where six columns no longer fit rather than
growing a horizontal bar. The command that ends the screen sits on the card's
bottom edge either way, so adding the fourth variable cannot carry it out of view
and neither screen scrolls as a whole; the status line stays where it is. The room
a scrollbar needs is left standing whether or not one is drawn, so the columns
beside it do not shift when the row that brings the bar arrives. The size and the position the
window was left in are remembered between runs, in
`$APPCONFIG/.window-state.json`; the window opens hidden and is shown once that
geometry has been applied, so it never flashes at the configured default first.
That default is the first run's alone, and it is picked so that run needs no
scroll: mode 1 lays its fields out in as
many columns as its width allows, so the default is a width wide enough for three
of them and a height the whole card fits in. It is centered, since a window left
where the platform puts one starts in the top-left corner.

The window has no native caption — it is undecorated, and the titlebar is a strip
of our own: the brand at the left, a drag area taking the rest of the row, and the
minimize, maximize/restore and close buttons at the right. Those are drawn with the
glyphs Windows uses for its own caption buttons, so the strip reads as a titlebar
rather than as a web page wearing one, and a double click on the drag area
maximizes the way a caption's does. Decorations are left out of the saved window
state on purpose: a state file written while the window still had the native
caption would put that titlebar back above ours. The strip also opens the two
cards the shell has besides the two modes. They sit to the left of its own buttons
rather than beside the brand, the one a person goes back to nearest the corner:
Settings, whose fields are the theme and the language above, and About outside it —
the version, which the badge copies to the clipboard
when it is clicked, since the number answers a question rather than being looked at
(it is read from the workspace `Cargo.toml`, the same number `build.rs` holds
against the copy in `tauri.conf.json` that Windows reads, so the card and the
file's own properties cannot drift apart),
the sizes of the runners embedded in this binary, and the license, author and
repository lines. Each of those three is the page it names: the click hands the
address to the backend, which opens it in the browser instead of letting the webview
navigate away from the app and leave the card behind the page, and which refuses any
host the card does not print itself. Where the words are not the address, the tooltip
says where the link goes. The last row names the folder the app's own files live in —
what a report of a strange build starts by asking for — and opens it. Both are cards
of the same shape a result uses, and both apply and dismiss without an Apply button
anywhere. Each names itself at the top, since a dialog's title only reaches a screen
reader and a card with nothing over its questions reads as a piece of the screen
behind it; in Settings an answer is the whole line rather than the dot beside it,
because a line is what a pointer aims at, and the chosen one says so with the accent
around that line.

Every Browse button opens where its own field last looked, so picking the folder
for shims and then a target does not send the next folder dialog to wherever the
target's ended. Each of them says what it does by shape — the folder glyph the About
card's own folder row wears — because the word only took the width the path needs,
and it stays as the tooltip and the accessible name. The directory a pick left behind is kept per field in
`$APPCONFIG/.pickers.json`, beside the window's geometry, and it survives a
restart: a field that still holds a path starts there, since what is typed now
says more than an older pick, and a remembered folder that no longer exists is
not a starting point either — the dialog then falls back to the system default.
Keys name what is picked rather than which screen picked it, so the two modes
share one Destination memory. The file is a convenience, never a dependency:
unreadable or garbled it reads as empty, and no dialog is refused because of it.

### 5.1 Mode 1: single shim

* **Source path**: file picker for the target `.exe`.
* **Destination**: folder picker, never prefilled. The pair belongs somewhere on
  `PATH` (6), not next to the binary it shims, where `<alias>.exe` would collide
  with the real one and the scan would read the folder as its own output.
* **Shim name / alias**: text input, prefilled from the target's stem, editable.
  A trailing `.exe` is dropped from what is typed, since a pasted file name is
  the common answer and `git.exe.exe` would only be a surprise later, and a name
  Windows could not create — a reserved device name, a character a file name
  cannot hold, a trailing dot or space — is refused with the same message as an
  empty one.
* **Default arguments**: single input field, split into `arg` lines on write.
* **Runner type**: dropdown `Auto detect` (default) / `CLI (Console)` /
  `GUI (Detached)` -> selects the embedded runner variant. No config field.
  See 5.4.
* **Working directory**: prefilled with the target's own folder, which is the
  default the runner falls back to (3.4), and editable from there. Writing the
  default back into the config would pin a path meant to keep following the
  target, so the pair is only given a `workdir` when the field names a different
  folder — case and a trailing separator do not make one different, and the line
  under the field says which of the two the config will hold.
* **Environment variables**: one row per variable — its name, where its value
  comes from (`Value`, `Secret`, `File`, 3.5), and the control that kind needs:
  a field written into the config as typed, a picker over the stored names, or a
  file path with the same folder-glyph button beside it, which opens a file
  picker over any file rather than one filtered to executables. Adding and
  removing a row carry a
  glyph rather than a word — `+` on the line naming the block, `−` at the end of
  each row — because the shape is the same in every language and a caption would
  set the column every row ends at wider than the rows need. A picker's
  placeholder names the field in the closed control and is left out of the list
  that opens from it.
  The names live in a card of their own, reached only from a row whose kind is
  `Secret`: the last line of that picker opens the card, and says
  `Save a first secret…` while the store is empty, since an empty store is the one
  that leaves the picker without a choice. A button under the rows would point at
  the card from a row that cannot use it. In the card a name is the line, so it
  keeps its left edge and the one action that destroys something takes the end:
  Windows' own trash rather than the `−` a row wears, because the two shapes look
  alike and mean very different things. Because a token is created once and then
  referenced by many shims, storing or forgetting one updates every picker at
  once, a name stored from the card a row opened lands in that row, and a name
  forgotten while a row points at it drops out of it rather than being written
  into a config that could not resolve it.
  Mode 1 only: the batch policy is no per-entry configuration (5.2).
* **Generate**: write `<destination>\<alias>.shim` then `<destination>\<alias>.exe`,
  in that order (5.3); on success, a dialog over the form naming the pair, its
  resolved runner type and the destination, with "Open destination folder" and
  "Edit PATH…" beside Close, and a reminder that the destination must be on
  `PATH` (6). Asked first, that reminder is only shown when the folder is not
  already an entry of the variable this window was started with; when it is, the
  dialog says so and offers no editor to open.
* Validates before writing: source exists, destination directory exists, alias
  is a legal file name. The button waits for those three fields
  to be filled rather than answering an incomplete form with an error; what the
  form cannot know without looking — a name already taken, a path that stopped
  existing — still arrives as one.

### 5.2 Mode 2: batch

* **Source directory**: folder picker.
* **Include subdirectories**: checkbox.
* **Discovered binaries table**: `Select` checkbox, `Shim name` (the file stem
  the pair is named after, so two rows sharing it are visible as such),
  `Executable` path (shortened in the middle, so the file name survives; the
  full path is the tooltip), `Version` (from the PE version resource, 5.5; the
  full string is the tooltip), `Size`, `Runner type` (the variant Auto resolved
  to, 5.4). `Select all` / `Select none`, how many were found and how many are
  selected. A `Filter` box above the table narrows it by name or path; it only
  hides rows, so the selection counter is what says how many shims a Generate
  would write. The table and this toolbar are a scan's output, so neither is on
  screen before the first one, and a scan that finds no executable says so in
  words instead of leaving the screen as it was. The two selection buttons are
  disabled whenever the rows on screen already match them, and Generate waits
  for a destination and one selected row: the scan can fill the table without
  either, and writing cannot start from that.
* **Destination directory**: folder picker.
* **Left out of the scan**: an `.exe` that has a sibling `.shim` is already a
  shim, and everything under the destination folder is where the pairs are
  being written — a tool tree that is also its own output folder is the normal
  case, not a corner. Changing the destination rescans.
* **Same name, one choice**: files that would claim one shim name are
  alternatives, not a set of checkboxes, so checking one clears the rest of its
  group and the table marks the group with a left band and an indent. The row
  the scan preselects is the one with the highest `FileVersion`, falling back to
  the modification date when a file reports no version at all — which is common
  enough (static builds of `ffmpeg` report none) that the date is the rule's
  second half rather than an edge case. `Select all` therefore means one row
  per name, never the whole table. A rescan answers with its own preselection,
  so choices made by hand are carried across it by path: one that is still in
  the results keeps its row and clears the rivals that claim its name, and a
  name whose chosen file has left the results keeps the scan's pick instead.
* **Policy**: no per-entry configuration. Each selected binary gets a config
  with `path` only, i.e. default arguments and default working directory, and
  the runner variant resolved by Auto.
* **Generate shims**: progress indicator ("Writing 12 of 25…") then a summary
  listing what was created and what was skipped with the reason, plus the same
  `PATH` reminder (6).

### 5.3 Collision policy (new, was undefined in draft 1)

* Never overwrite silently. If `<alias>.exe` or `<alias>.shim` already exists in
  the destination, the entry is **skipped** and reported. In Mode 1 the skip is
  the failure dialog, in Mode 2 it is a row in the summary.
* Inside Mode 2, two discovered binaries may share a stem (recursive scan of
  `tools\x\git.exe` and `tools\y\git.exe`). First one wins; every later one is
  reported as skipped with a `duplicate` reason, kept distinct from `collision`,
  which means a file already sitting in the destination. A row whose file has
  left the disk since the scan is skipped as `missing`, and one the write itself
  refused as `write`, so the summary's reasons are the same codes mode 1 reports
  as failures. Disambiguating suffixes
  are deliberately not generated — a skipped-and-reported batch is honest, a
  silently renamed one is not.
* The generated `.exe` is written **after** a successful `.shim` write, so a
  partially generated shim is a working config without a runner rather than a
  runner that reads a stale config. (Revisit if the write order proves awkward.)

### 5.4 Runner type selection

The variant is chosen from the target binary's PE header, in the GUI's Rust
backend. The runner stays unaware of the mechanism.

* **Auto detect** reads `OptionalHeader.Subsystem` from the target:
  `IMAGE_SUBSYSTEM_WINDOWS_GUI` (2) -> GUI variant,
  `IMAGE_SUBSYSTEM_WINDOWS_CUI` (3) -> CLI variant. Anything else, or a file
  whose header cannot be read (not a PE, truncated, unusual layout), falls back
  to CLI — the variant that at least reports its errors.
* Detection runs when a source is picked (Mode 1) and while the directory is
  scanned (Mode 2), so the resolved type is visible **before** generating.
* A forced choice is honoured as given, even where it is wrong: forcing GUI for
  a console target produces a detached process whose output goes nowhere, and
  forcing CLI for a GUI target produces a console that flashes. That is the
  point of exposing the override, so no warning dialog is added on top of it.
* Mode 2 has no override control: the batch policy is Auto for every row, and
  the `Type` column is what makes that auditable.

### 5.5 File version

Also read from the file, never from running it: the `Version` column and the
preselection of 5.2 come from the PE resource directory.

* The walk is `IMAGE_DIRECTORY_ENTRY_RESOURCE` -> `RT_VERSION` -> the single
  named resource -> the single language block -> `IMAGE_RESOURCE_DATA_ENTRY`,
  whose `OffsetToData` is an RVA resolved through the section table. RVAs are
  bounded by the section's raw size, so a malformed header cannot make the scan
  read past what the file holds.
* Two fields are read out of `VS_VERSIONINFO`, and they do not always agree:
  `VS_FIXEDFILEINFO.dwFileVersion`, which is what gets compared, and the
  `StringFileInfo` `FileVersion`, which is what is shown because it is what the
  tool itself and Explorer print. A file that reports only one of the two is
  normal; the display string falls back to the four numbers.
* Nothing is ever failed over a version: no resource, no PE, unreadable file all
  mean an empty cell and the modification date deciding 5.2.

### 5.6 Update check

The manager asks GitHub whether a newer release exists, at most once a day, and
says so in a card. This is the only network request the app makes.

* **Who asks**: the webview, with a plain `fetch` of this repository's
  `releases/latest`, which answers any origin. The window already is a browser,
  so no HTTP or TLS client is added to the app for one public JSON file; the
  content policy names `https://api.github.com` under `connect-src` and nothing
  else may be reached.
* **Who decides**: the Rust side, because that is where a comparison is worth a
  unit test. `update_due` answers whether a day has passed since the last ask;
  `update_seen` records the ask and compares the tag against the running
  version — a leading `v` is a tag's spelling and a `-rc.1` label is not a
  number, so only a bigger version counts as newer. The one number of state
  lives in `$APPCONFIG/.update.json` beside the theme and the pickers' folders,
  and it is written whether or not the ask was answered: a machine with no
  network is not served by asking again on every start.
* **The card**: the same shape Settings and About use, and the third one of the
  shell — the only one nobody opens. The two version numbers in one line, the
  release notes as their
  author wrote them (plain lines, no markdown renderer added for the sake of
  bullets), and *Open release*, which hands the release page to `open_link` —
  the same host allow-list the About card's links pass through, so an answer
  from the network cannot point the browser anywhere else. *Close* is the
  focused button, as in every other card.
* **When**: two seconds after the window is up, so a check is never what stands
  between a person and the form they opened the app for.
* **Silence**: a refused request, a 404 and a tag that is not a number all bring
  no message, and no error dialog is offered for a network nobody was asking
  about. Nothing identifies the machine: the request is the same one a browser
  makes for a release page, and GitHub counts it as such — which is also why the
  day between asks is the point rather than the 60-an-hour limit.

### 5.7 Runner refresh

A shim is a copy of a runner, so a fix to a runner reaches none of the shims made
before it. This is the way to bring them up without deleting the configs that say
what each shim is.

* **Who asks**: a person, from the third tab. Nothing here runs on its own, because
  the app is not a service and an unasked write to files on `PATH` is not what a
  generator owes anybody. No list of made shims is kept either: a pair says where it
  is, and the two ways to ask for one are the folder in the field and, while that
  field is empty, every folder the variable `folder_on_path` (5) already reads.
* **Which files are ours**: the `.exe` answers for the pair, from the `ProductName`
  its own `VERSIONINFO` carries (2.2) — a name no other tool's binary has any reason
  to write, and one the hand-written PE reader already walks the resource tree to
  reach for *FileVersion*. A `.shim` in the same folder makes a pair only once that
  answer says shimbox, because `%PATH%` reaches Scoop's own `shims` folders, which
  hold `git.shim` beside `git.exe` in exactly the shape a scan of a folder asks for.
  A config with no `.exe` beside it is taken at its own word instead — the header
  line of 3.2, or a `runner` line of the shape only this manager writes — since a
  half pair is the one case with nothing left to ask. The guard runs again over the
  bytes as they are when *Update runners* is clicked, not only during the scan.
* **What counts as stale**: the copy on disk against the bytes this build embeds,
  byte for byte. A version number cannot answer this — the runners are rebuilt while
  `CARGO_PKG_VERSION` stays where it was, which is how a fix to a runner reaches no
  shim today — and the number the file carries in its own resource is the same
  answer with the same hole. The `runner` line of the config is therefore shown as
  *Made with* rather than trusted: a pair made before the line existed, or made by a
  build that wrote a different number, is listed by what its bytes are, not by what
  its line says.
* **Which runner**: the PE header of the copy on disk, for the same reason 5.4 reads
  a target's header. The config's own line decides only when there is no header to
  read, which is the half-pair 5.3 leaves on purpose. A refresh therefore never
  turns a GUI shim into a console one, or the other way round.
* **What is written**: the `.exe` half, and the one `runner` line of the config that
  says which bytes are in it. Every other line goes back exactly as it came, values
  and endings included — the config is what the person wrote, and re-writing it from
  parsed fields would quietly drop whatever the parser does not know. Bytes first,
  then the line: the other order would leave a pair claiming to be current while its
  runner is still the old one, and a stale row is the worse answer by far.
* **What is not**: no pair is deleted, no `.exe` without a config is touched (it is
  somebody's program, not a shim), and the collision rule of 5.3 stands untouched,
  because that rule is about making a pair rather than keeping one.
* **Failure**: a shim that is running now cannot be written over, and Windows says
  so. That answer belongs to its own row, so the rest of the list is refreshed and
  one name is reported. A pair that stopped being ours since the scan answers on its
  own row too, and keeps every byte it has. The summary is the same shape mode 2
  already has.

---

## 6. Non-Functional Requirements

* **Performance**: no noticeable latency in scripts and pipes; see 2.2 targets.
* **Portability**: GUI distributed as a single portable `.exe` or a light
  installer.
* **`PATH` is maintained by the user, by hand.** A shim directory is only
  usable once it is on `PATH`, and v1 neither adds, removes nor validates it.
  It is read, and only to decide what a result says: the destination is compared
  with the entries of the variable the manager process was started with, so an
  entry added while that window stayed open is not seen and the reminder is
  repeated rather than withheld. The user adds the destination directory in the system's own
  variable editor — reached from *Settings → System → About → Advanced system
  settings → Environment Variables*, from `setx` in a terminal, or from the
  "Edit PATH…" action of a successful
  result, which starts that editor rather than writing the variable itself — and
  a shim invoked before that fails with "not recognized": expected, not an error
  to report. The reminder in the success message of both modes says what is
  still to do, and the README has a `Setup` section with those exact steps.
* **Antivirus**: the expected shape (small unsigned exe, `CreateProcess`-heavy
  imports, renamed to N different names) is a realistic SmartScreen / Defender
  false-positive trigger, and it is not made worse or better by where the config
  lives. Mitigations, in order of effect: an OV/EV code-signing certificate
  (self-signing achieves nothing), distribution volume/reputation, and
  false-positive submissions. No signing is committed in v1.

---

## 7. Roadmap

| Stage | Milestone | Deliverables |
| :--- | :--- | :--- |
| **1** | Core runner | Both variants build; config parser; raw command-line forwarding; stdio inheritance; exit codes; CLI stderr + GUI MessageBoxW. Verified from a terminal before any GUI exists. |
| **2** | Runner tests | Scripted checks: an argument that survives `raw_arg` untouched (one holding a double space, which re-serialising would split), exit-code propagation, missing/invalid config, interactive stdin, workdir default. |
| **3** | Tauri scaffold | Tauri v2 + frontend template, both runners embedded, `build.rs` staging and watching the runner bytes so a stale embedding is a build error. |
| **4** | Mode 1 | Pickers, alias, arguments, workdir, validation, generation, collision skip, notification. Includes PE `Subsystem` detection (5.4) behind the `Auto detect` dropdown. |
| **5** | Mode 2 | Directory scan, table with checkboxes, detected `Type` and file version (5.5), a filter, one choice per shim name, batch write, progress, summary with skip reasons. |
| **6** | Polish | Size/flag pass on the runner, error dialog copy, README with the `Setup` / `PATH` steps. |
| **7** | Child environment | `env` and `envref` in the config, resolved at launch from a file or a generic credential (3.5, FR-1.8); the row editor and the secret store in mode 1. |
| **8** | Release pipeline | `scripts/package.ps1` making one archive, and a workflow that runs the build, the tests and the smoke checks on a Windows runner, with the archive on a draft release for a person to publish. |

---

## 8. Open Decisions & Deferred Work

Decisions still open:

1. **Hard links instead of copies** for Mode 2: saves space and makes runner
   updates land on all shims at once, but requires FR-1.1 to key off `argv[0]`.
   Only about the generated shims: the staged manager in `bin\` is a link, and
   the runners stay copies because the manager embeds their bytes.
   It is a question about space now rather than about updates: 5.7 answers the
   second for a copy, and what a link would cost is a shim that cannot be put on a
   desktop or carried to another machine by itself.
2. **A backslash-quote case in the raw command-line round trip**: the reason
   FR-1.3 exists, and so far covered only by the parser's own unit test. What a
   child is handed is asserted through an argument with a double space, which is
   the half of it a `cmd.exe` can echo back honestly.

Resolved since draft 3:

* **How a release gets built and shipped**: one archive, built by the pipeline
  that now exists. `.github/workflows/ci.yml` runs the three masters already in
  `scripts/` — it invents no fourth build path — on a `windows-latest` runner
  under Windows PowerShell, which is the shell their `#Requires` line asks for.
  There the size target of 2.2 stops being a line in a report and becomes a
  failed build (`build.ps1 -StrictSize`). `package.ps1` then wraps the manager,
  the README and the license into `bin\shimbox-<version>-windows-x64.zip`, and
  reads the version from the workspace manifest so a `v*` tag that disagrees with
  what was built stops the package. A tagged push uploads the archive to a
  **draft** release: publishing it, and writing the notes the app's own update
  card shows (5.6), stays a human step, because that text is a promise to a
  reader rather than something a pipeline can invent. Nothing here needs the
  remote to be GitHub — the workflow is inert until the repository moves there,
  and every step of it runs locally the same way.
* **`.shim` file extension**: the collision with Scoop that this question feared
  is real — `scoop-shim` uses the same extension for the same idea — and it
  turned out to cost nothing, because the two formats cannot be misread as each
  other in either direction and both failures name themselves (3.6). Kept.
* **Frontend**: Vanilla JS, no build step and no dependency the running app reads,
  reached through `withGlobalTauri`. Its one npm file is Prettier, which says how
  the UI is wrapped and is installed only to check that: `build.ps1` opens with the
  two formatting gates, Rust and then the UI, and a missing install is a named setup
  step rather than a check skipped - the drift this repository had already collected
  once in ten files is what that refuses to let happen again. Every string goes
  through a `data-i18n` key and a
  per-language dictionary file, so a language is one file in `ui/locales/` plus
  three lines in `ui/i18n.js` — the import, the entry in `languages`, the entry
  in `dictionaries`; English and Russian are both registered, and the reader
  picks between them (5).

Deferred, explicitly out of v1:

* Shimming `.bat` / `.cmd` / `.ps1` targets (needs `cmd.exe /c` or `ShellExecute`).
* `path` relative to the config file, for portable tool sets that move between
  drives.
* Listing, editing, re-pointing and deleting existing shims.
* Per-entry configuration in Mode 2, including the child environment: a value
  that differs per shim has nowhere to go in a table of checkboxes, so the batch
  policy stays `path` only (5.2) and mode 1 carries the whole editor. Opening a
  row for its own arguments, working directory and environment is the way in,
  and the environment is the reason to want it.
* `PATH` management from the UI.
* Single-file shims with the config appended to the exe (rejected in review:
  needs a payload container format and self-reading code, reads to behavioural
  heuristics like a dropper, and forbids hard links).
* Cross-platform (the config keys are Windows-shaped only in that `path` points
  at an `.exe`; the format itself has nothing to change).
