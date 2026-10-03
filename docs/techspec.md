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
  .gitignore            /target, /bin, node_modules/, dist/
  bin/                  build output, ignored (runner-cli.exe, runner-gui.exe)
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
    gui/                Tauri app
      src-tauri/        Rust backend
      ui/               frontend sources
      dist/             built frontend (generated, ignored)
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
  * Backend: Rust (Tauri v2). Frontend: undecided between Vanilla and Svelte
    (section 8); the UI is two tabs and one table, so no build-chain-heavy stack
    is needed for its sake.
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
  | otherwise | child's code | Cannot be distinguished from a child that itself returned 101–103; accepted. |

* **FR-1.6 Error reporting**: on any runner-side failure, the CLI variant writes
  one line to stderr naming the config file and the reason; the GUI variant
  shows a `MessageBoxW` with the same text, since it has no console.
* **FR-1.7 No fallback discovery**: if the config is missing or `path` is not
  found, the runner fails as above. It does not search `PATH`, does not guess,
  and does not prompt.

---

## 5. Functional Requirements: `shim-gui`

Two tabs, no other navigation.

### 5.1 Mode 1: single shim

* **Source path**: file picker for the target `.exe`.
* **Destination**: folder picker.
* **Shim name / alias**: text input, prefilled from the target's stem, editable.
* **Default arguments**: single input field, split into `arg` lines on write.
* **Runner type**: dropdown `Auto detect` (default) / `CLI (Console)` /
  `GUI (Detached)` -> selects the embedded runner variant. No config field.
  See 5.4.
* **Working directory**: optional folder picker; empty means the default from
  3.4.
* **Generate**: write `<destination>\<alias>.exe` then `<destination>\<alias>.shim`;
  on success, a notification with an "Open destination folder" action and a
  reminder that the destination must be on `PATH` (6).
* Validates before writing: source exists, destination directory exists and is
  writable, alias is a legal file name.

### 5.2 Mode 2: batch

* **Source directory**: folder picker.
* **Include subdirectories**: checkbox.
* **Discovered binaries table**: `Select` checkbox, `Binary Name`, `Path`,
  `Type` (the runner variant Auto resolved to, 5.4). `Select All` /
  `Deselect All`.
* **Destination directory**: folder picker.
* **Policy**: no per-entry configuration. Each selected binary gets a config
  with `path` only, i.e. default arguments and default working directory, and
  the runner variant resolved by Auto.
* **Generate All**: progress indicator ("Generated 12/25…") then a summary
  listing what was created and what was skipped with the reason, plus the same
  `PATH` reminder (6).

### 5.3 Collision policy (new, was undefined in draft 1)

* Never overwrite silently. If `<alias>.exe` or `<alias>.shim` already exists in
  the destination, the entry is **skipped** and reported. In Mode 1 the skip is
  an error shown next to the form, in Mode 2 it is a row in the summary.
* Inside Mode 2, two discovered binaries may share a stem (recursive scan of
  `tools\x\git.exe` and `tools\y\git.exe`). First one wins; every later one is
  reported as skipped due to name collision. Disambiguating suffixes are
  deliberately not generated — a skipped-and-reported batch is honest, a
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

---

## 6. Non-Functional Requirements

* **Performance**: no noticeable latency in scripts and pipes; see 2.2 targets.
* **Portability**: GUI distributed as a single portable `.exe` or a light
  installer.
* **`PATH` is maintained by the user, by hand.** A shim directory is only
  usable once it is on `PATH`, and v1 neither adds, removes, validates nor
  inspects `PATH`. The user adds the destination directory through
  *Settings → System → About → Environment Variables* (or `setx`), and a shim
  invoked before that fails with "not recognized" — expected, not an error to
  report. The only UI acknowledgement is the reminder in the success message of
  both modes, and a `Setup` section in the README with those exact steps.
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
| **5** | Mode 2 | Directory scan, table with checkboxes and detected `Type`, batch write, progress, summary with skip reasons. |
| **6** | Polish | Size/flag pass on the runner, error dialog copy, README with the `Setup` / `PATH` steps. |

---

## 8. Open Decisions & Deferred Work

Decisions this draft still needs:

1. **Frontend**: Vanilla vs Svelte.
2. **`.shim` file extension**: worth checking it does not collide with an
   existing convention before shipping — unverified suspicion, not a finding.
3. **Hard links instead of copies** for Mode 2: saves space and makes runner
   updates land on all shims at once, but requires FR-1.1 to key off `argv[0]`.

Deferred, explicitly out of v1:

* Shimming `.bat` / `.cmd` / `.ps1` targets (needs `cmd.exe /c` or `ShellExecute`).
* `path` relative to the config file, for portable tool sets that move between
  drives.
* Listing, editing, re-pointing and deleting existing shims.
* `PATH` management from the UI.
* Single-file shims with the config appended to the exe (rejected in review:
  needs a payload container format and self-reading code, reads to behavioural
  heuristics like a dropper, and forbids hard links).
* Cross-platform (the config keys are Windows-shaped only in that `path` points
  at an `.exe`; the format itself has nothing to change).
