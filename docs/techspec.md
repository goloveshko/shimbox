# Technical Specification: Shim Manager (Desktop GUI)

Status: draft 3. Draft 1 was reviewed and its findings folded in; the remaining
open items are listed in section 8.

## 1. Project Overview

A lightweight executable-shimming tool for Windows: it generates tiny proxy
executables that live in a directory on `PATH` and forward invocations to the
real binaries. Two modules:

1. **`shim-runner`**: a minimal Rust executable, shipped in two variants
   (console / windows subsystem). Reads its companion config file and launches
   the target with the caller's arguments.
2. **`shim-gui`**: a Tauri desktop app that generates individual shims and
   batch-generates them for a whole directory.

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

The variant is encoded in the bytes written to `<alias>.exe`. **It is not a
config field** — the runner contains no branch on it, and the GUI has no reader
for it. The runner type dropdown in the UI (5.4) selects which embedded binary
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

### 2.3 Repository layout

The root stays small: build tooling and generated artifacts live below it.

```text
shimbox/
  Cargo.toml            workspace manifest
  Cargo.lock
  README.md
  .gitignore            /target, /bin, tauri's generated schemas
  bin/                  build output, ignored
                        (runner-cli.exe, runner-gui.exe, shimbox.exe)
  docs/
    techspec.md
  scripts/
    common.ps1          shared helpers, dot-sourced only
    build.ps1           master: tests + both variants + size report
    smoke.ps1           end-to-end runner checks against real binaries
    build.cmd           thin wrappers, no logic
    test.cmd
    smoke.cmd
  crates/
    runner/             shim-runner, both variants
  apps/
    gui/
      assets/           icon source
      src-tauri/        Rust backend, embeds ui/ and both runners
      ui/               frontend: no build step, no npm dependencies
        index.html
        main.js         tab switching, status line
        i18n.js         language resolution and lookup
        form.js         widgets both screens build the same way
        app.css
        locales/        one file per language
        screens/        one file per mode
```

Rule of thumb: nothing new is added to the root directory without a reason;
scratch scripts go to `scripts/`, notes to `docs/`.

### 2.2 Components

* **`shim-runner` (crate `runner`)**
  * Windows-only in v1.
  * No third-party crates in the runtime path: the config parser is hand-written
    (section 3), and the two Win32 calls that are needed (`GetCommandLineW`,
    `MessageBoxW`) are declared as `extern "system"` rather than pulled in via
    the `windows` crate.
  * Size target: **<= 400 KB** per variant with `opt-level = "z"`, `lto = "fat"`,
    `codegen-units = 1`, `panic = "abort"`, `strip`. Draft 1 targeted 150 KB,
    which is not reachable while linking `std`; going below it requires
    `-Z build-std` or dropping `std`.
  * Added latency target: **<= 3 ms** over a direct invocation. Process creation
    alone costs roughly 0.5–1.5 ms, so anything under ~3 ms is unmeasurable
    against the noise.

* **`shim-gui` (crate / Tauri app `gui`)**
  * Backend: Rust (Tauri v2). Frontend: Vanilla JS with no build step and no npm
    dependencies (section 8); the UI is two tabs and one table, so no
    build-chain-heavy stack is needed for its sake.
  * Embeds **both** runner variants via `include_bytes!` and writes the selected
    one.
  * Dev loop: the embedded bytes go stale against a freshly built runner, so the
    workspace is ordered (`build.rs` or a cargo workspace dependency) so the GUI
    always embeds the just-built runner. In debug builds the runner is read from
    `target/` instead of being embedded.

---

## 3. Configuration Format (`<alias>.shim`)

Every generated `<alias>.exe` requires an adjacent `<alias>.shim` with the same
base name.

### 3.1 Why not JSON

Draft 1 left the format as "JSON or INI". JSON was incompatible with the
zero-dependency rule and with the size target, and it would need a parser in two
places. The format is therefore a flat `key=value` file, ~40 lines of
hand-written parsing in the runner. The GUI **writes** this format and never
parses it, so only one parser exists.

### 3.2 Syntax

```ini
# shimbox config
path=C:\Tools\ffmpeg\bin\ffmpeg.exe
arg=-hide_banner
workdir=C:\Tools\ffmpeg
env=FFMPEG_DEBUG=1
envref=GITHUB_TOKEN=cred:shimbox/github
```

* UTF-8, no BOM, CRLF or LF accepted. Lines are split on the first `=`.
* The value is the remainder of the line **verbatim**, with only a trailing
  `\r` removed. Values are not quoted, escaped or trimmed — backslashes and
  spaces are literal, so no escaping rules exist to get wrong.
* `#` at the start of a line (after optional whitespace) is a comment.
* Unknown keys are ignored.

### 3.3 Keys

| Key | Required | Meaning |
| :--- | :--- | :--- |
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

* Credentials are named under a `shimbox/` prefix and the GUI only offers names
  it wrote itself, so a config cannot point the runner at another program's
  stored secret. A blob is read as UTF-8, which is what shimbox writes, and as
  UTF-16 when it holds a NUL, which is what `cmdkey` writes.
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

---

## 4. Functional Requirements: `shim-runner`

* **FR-1.1 Self-identification**: the runner derives its own stem and directory
  from `std::env::current_exe()`. Hard links are not used in v1 (section 8);
  with hard links this must switch to `argv[0]`, because `current_exe()` may
  return any of the link names and therefore the wrong config file.
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
  | config file missing | 101 | |
  | config unreadable, or `path` absent/empty | 102 | |
  | spawn failed | 103 | Last Win32 error is included in the message. |
  | an `envref` that names nothing readable | 104 | The child is not started: a target that expects a value it never gets fails later and less legibly. |
  | otherwise | child's code | Cannot be distinguished from a child that itself returned 101–103; accepted. |

* **FR-1.6 Error reporting**: on any runner-side failure, the CLI variant writes
  one line to stderr naming the config file and the reason; the GUI variant
  shows a `MessageBoxW` with the same text, since it has no console.
* **FR-1.7 No fallback discovery**: if the config is missing or `path` is not
  found, the runner fails as above. It does not search `PATH`, does not guess,
  and does not prompt.
* **FR-1.8 Child environment**: the `env` pairs are applied in the order the
  file holds them, and the `envref` ones after them, each resolved at launch
  (3.5), so a reference wins over a literal of the same name and the last of two
  like it wins. Nothing is removed from the inherited environment: the child
  sees the caller's variables plus these.

---

## 5. Functional Requirements: `shim-gui`

Two tabs, no other navigation. Both screens are built once and a tab only shows
one of them, so a half-written form and a scan someone is still ticking survive a
look at the other mode — nothing is re-asked and nothing is cleared by switching.

The shell reads as a Windows application rather than a web page: the window sits
on a Mica backdrop and the interface is semi-opaque cards over it. No colour is
picked by hand — each one is a token declared once per theme, and the accent is
`Highlight`, the colour the user chose in the system. Which of the two themes is
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
before the interface speaks it. It applies by building the window again rather than
by rewriting the text where it stands: the strings are read once, as each screen is
made, so a live swap would have to remember which node came from which key. That
rebuild discards a half-written form, which is what the field's own hint says, and
it is why the theme, which needs no such thing, does not work the same way. The row
is only there while there is a choice to make, so a single-language build shows the
card with its one field again. A field
is one block of caption, control and hint, and the blocks reflow from two
columns to one with the window. A result is reported in a dialog over the form
rather than below it: the form is often taller than the window, and whether the
write worked is the last thing in it that should need a scroll to be found. The
dialog is a card over a frame that dims the whole window, so a click anywhere
outside the card ends the result; `Esc` and its own Close action do the same, and
the body is the part that gives when a batch summary is longer than the window. A
result that wrote something also offers the two ways to act on the folder it
wrote into: open it, and open the system editor that puts it on `PATH` (6).
The batch table takes the height the rest of the screen leaves it and scrolls
inside its own card, under a header that stays put; below the width where six
columns no longer fit it drops Version and Size rather than growing a horizontal
bar, so that screen never shows two scrollbars at once. A screen that is simply
taller than the window — mode 1 with three environment rows is — scrolls as a
whole, and the status line below stays where it is. The size and the position the
window was left in are remembered between runs, in
`$APPCONFIG/.window-state.json`; the window opens hidden and is shown once that
geometry has been applied, so it never flashes at the configured default first.
That default is the first run's alone, and it is picked so that run needs no
scroll: mode 1 is the taller of the two screens, and it lays its fields out in as
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
cards the shell has besides the two modes: Settings, whose fields are the theme and
the language above, and About — the version, the sizes of the runners embedded in
this binary, and the license, author and repository lines. Both are cards of the
same shape a result uses, and both apply and dismiss without an Apply button
anywhere.

Every Browse button opens where its own field last looked, so picking the folder
for shims and then a target does not send the next folder dialog to wherever the
target's ended. The directory a pick left behind is kept per field in
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
* **Child environment**: one row per variable — its name, where its value comes
  from (`Value`, `Secret`, `File`, 3.5), and the control that kind needs: a field
  written into the config as typed, a picker over the stored names, or a file path
  with a folder button beside it. Adding and removing a row carry a glyph rather
  than a word — `+` on the line naming the block, `−` at the end of each row —
  because the shape is the same in every language and a caption would set the
  column every row ends at wider than the rows need. A picker's placeholder names
  the field in the closed control and is left out of the list that opens from it.
  The names live in a card of their own, reached only from a row whose kind is
  `Secret`: the last line of that picker opens the card, and says
  `Save a first secret…` while the store is empty, since an empty store is the one
  that leaves the picker without a choice. A button under the rows would point at
  the card from a row that cannot use it. Because a token is created once and then
  referenced by many shims, storing or forgetting one updates every picker at
  once, a name stored from the card a row opened lands in that row, and a name
  forgotten while a row points at it drops out of it rather than being written
  into a config that could not resolve it.
  Mode 1 only: the batch policy is no per-entry configuration (5.2).
* **Generate**: write `<destination>\<alias>.shim` then `<destination>\<alias>.exe`,
  in that order (5.3); on success, a dialog over the form naming the pair, its
  resolved runner type and the destination, with "Open destination folder" and
  "Edit PATH…" beside Close, and a reminder that the destination must be on
  `PATH` (6).
* Validates before writing: source exists, destination directory exists and is
  writable, alias is a legal file name. The button waits for those three fields
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
* **Generate All**: progress indicator ("Writing 12 of 25…") then a summary
  listing what was created and what was skipped with the reason, plus the same
  `PATH` reminder (6).

### 5.3 Collision policy (new, was undefined in draft 1)

* Never overwrite silently. If `<alias>.exe` or `<alias>.shim` already exists in
  the destination, the entry is **skipped** and reported. In Mode 1 the skip is
  the failure dialog, in Mode 2 it is a row in the summary.
* Inside Mode 2, two discovered binaries may share a stem (recursive scan of
  `tools\x\git.exe` and `tools\y\git.exe`). First one wins; every later one is
  reported as skipped with a `duplicate` reason, kept distinct from `collision`,
  which means a file already sitting in the destination. Disambiguating suffixes
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

---

## 6. Non-Functional Requirements

* **Performance**: no noticeable latency in scripts and pipes; see 2.2 targets.
* **Portability**: GUI distributed as a single portable `.exe` or a light
  installer.
* **`PATH` is maintained by the user, by hand.** A shim directory is only
  usable once it is on `PATH`, and v1 neither adds, removes, validates nor
  inspects `PATH`. The user adds the destination directory in the system's own
  variable editor — reached from *Settings → System → About → Environment
  Variables*, from `setx`, or from the "Edit PATH…" action of a successful
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
| **2** | Runner tests | Scripted checks: quoting round trip through `raw_arg` (backslash-quote cases), exit-code propagation, missing/invalid config, interactive stdin, workdir default. |
| **3** | Tauri scaffold | Tauri v2 + frontend template, both runners embedded, workspace ordering so debug builds read the freshly built runner. |
| **4** | Mode 1 | Pickers, alias, arguments, workdir, validation, generation, collision skip, notification. Includes PE `Subsystem` detection (5.4) behind the `Auto detect` dropdown. |
| **5** | Mode 2 | Directory scan, table with checkboxes, detected `Type` and file version (5.5), a filter, one choice per shim name, batch write, progress, summary with skip reasons. |
| **6** | Polish | Size/flag pass on the runner, error dialog copy, README with the `Setup` / `PATH` steps. |
| **7** | Child environment | `env` and `envref` in the config, resolved at launch from a file or a generic credential (3.5, FR-1.8); the row editor and the secret store in mode 1. |

---

## 8. Open Decisions & Deferred Work

Decisions this draft still needs:

1. **`.shim` file extension**: worth checking it does not collide with an
   existing convention before shipping — unverified suspicion, not a finding.
2. **Hard links instead of copies** for Mode 2: saves space and makes runner
   updates land on all shims at once, but requires FR-1.1 to key off `argv[0]`.

Resolved since draft 3:

* **Frontend**: Vanilla JS, no build step and no npm dependencies, reached
  through `withGlobalTauri`. Every string goes through a `data-i18n` key and a
  per-language dictionary file, so a language is one file in `ui/locales/` plus one
  line in `ui/i18n.js`; English and Russian are both registered, and the reader
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
