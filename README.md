# shimbox

A tiny executable-shimming tool for Windows. It puts a small proxy `.exe` next
to a flat config file in one folder, you put that folder on `PATH`, and from
then on the tool answers to a name you chose — with the arguments it always
needs, the working directory it expects, and the environment variables it looks
for, all supplied without touching the original binary.

Typical uses: a portable tool that only runs from its own folder, a build of
`ffmpeg` you would rather call `video`, a program that must be started with
`-some-flag` every single time, or a CLI that has to be given a token your
scripts should not carry in plain text.

Two parts, one workspace:

| Part | What it is |
| :--- | :--- |
| `shim-runner` | A ~220 KB Rust executable built in two variants (console subsystem, windows subsystem). It finds the config named after itself, reads it, and launches the target with your arguments. |
| `shimbox` | A Tauri v2 desktop app that writes those pairs: one shim at a time, or a whole directory at once. |

v1 **generates**. It does not list, inspect, re-point or delete shims that
already exist — see `docs/techspec.md` 8 for that and the other deferred items.

## How a shim works

A shim is two files that share a base name, in the same folder:

```text
C:\Shims\video.exe     a copy of one runner variant
C:\Shims\video.shim    the config it reads at launch
```

The runner locates itself on disk, so `video.exe` reads `video.shim` beside it
and nothing else — renaming or moving the pair renames or moves the command.
Arguments you type are forwarded to the target byte-for-byte, stdin/stdout and
the exit code are inherited, and the console or window behaviour comes from
which variant was copied: the manager picks that from the target's PE header
unless you override it.

## Build

Windows only, and it needs the MSVC toolchain, `cargo` and the WebView2 runtime
(already present on Windows 11). The order matters: the manager embeds the
runner's bytes, so the runner is built and staged first. `scripts\build.cmd`
does the whole thing.

```powershell
scripts\build.cmd                     # both runner variants, tests, the manager
scripts\build.cmd -Profile debug      # the same, unoptimized
scripts\test.cmd                      # runner and tests, no manager
scripts\smoke.cmd                     # end-to-end runner checks against real binaries
```

Each `.cmd` is a two-line wrapper over `scripts\build.ps1` / `smoke.ps1`, which
is where the logic lives; `smoke.cmd` reads the runners from `bin\`, so build
first. Double-clicking a wrapper leaves the window open until you press a key —
`-NoPause`, or `SHIMBOX_NO_PAUSE=1` in the environment, skips that. The results
land in `bin\`:

```text
bin\runner-cli.exe   console-subsystem runner   (target: under 400 KB)
bin\runner-gui.exe   windows-subsystem runner   (target: under 400 KB)
bin\shimbox.exe      the manager
```

Building just the manager with `cargo build -p gui` fails on purpose until
`bin\` holds a runner, because embedding a stale runner is the one mistake this
layout makes easy to miss.

## Setup

1. Make a folder for the shims, for example `C:\Shims`.
2. Put it on `PATH`: *Settings → System → About → Advanced system settings →
   Environment Variables*, edit `Path`, add the folder. Every successful result
   in the manager has an **Edit PATH…** button that opens that same window.
3. Open `bin\shimbox.exe`, generate shims into the folder.
4. Start a **new** terminal — `PATH` is read when a process starts — and run the
   name you gave the shim.

Adding the folder to `PATH` is yours to do: shimbox opens the system editor for
convenience but never writes the variable itself. Until the folder is on `PATH`
the shim is simply not found, which is expected rather than an error.

## Using the manager

**Single shim** — pick the target executable, get its folder and file stem
offered as the destination and name, then adjust: alias, arguments, working
directory, runner variant, and the child's environment. The `Generate` button
waits until the target, the destination and the name are all filled in.
Arguments are written into the config exactly as you type them, with no quoting
added.

A variable's value is either typed into the config, read from a file at launch, or
a stored secret. Secrets go to Windows Credential Manager under `shimbox/<name>`.
The card holding them opens from a secret row's own picker — its last line reads
**Manage secrets…**, or **Save a first secret…** while nothing is stored yet — and
the name you store there goes straight into the row that asked for it. One card
serves every row, so a token is typed once and referenced by as many shims as need
it.

**Folder batch** — pick a source directory, scan it (subdirectories optional),
and tick what to shim. The table shows the detected runner type, the file
version and the size for every candidate, and a filter narrows it without
touching the selection. Files that would claim the same command name are
alternatives rather than a set of checkboxes: the newest is preselected and
choosing another clears its rivals. A rescan keeps what you had chosen by hand.
Batch shims get `path` only — no per-entry arguments or environment; open the
single-shim screen for those. A file that is already a shim is left out of the
scan, and a name that turns out to be taken is skipped with the reason printed
in the summary; the single-shim screen refuses to overwrite it and says so in
the dialog instead.

The result of either mode is a card you can dismiss by clicking anywhere around
it, with **Open destination folder** and **Edit PATH…** when something was
written.

Every Browse button reopens in the folder you last chose for that field — kept in
`$APPCONFIG/.pickers.json` and restored across restarts — so the destination
dialog no longer opens wherever the target's dialog ended.

The titlebar is ours rather than the system's: the name at the left, and at the
right **Settings**, **About**, minimize, maximize/restore and close. Drag the
window anywhere along the strip, or double-click it to maximize. **Settings**
offers Light, Dark or following Windows, and applies as you pick it — kept in
`$APPCONFIG/.settings.json`, so it is still there next run. Below it, the same
card asks which language to speak: English, Русский, or following Windows again.
A language applies by rebuilding the window, so anything still typed into the
form is lost — the field says so before you pick. **About** shows the
version — click it and it is on the clipboard — the sizes of the runners built into
this binary, and the license, author and repository lines, each of which opens in
your browser rather than in this window. The last row names the folder this app keeps
its own files in, which is the first thing a report of a strange build asks for, and
opens it.

## Configuration reference

`<alias>.shim` is UTF-8, one `key=value` per line, split on the **first** `=`.
Values are taken verbatim: no quoting, no escapes, no trimming, so backslashes
and spaces mean exactly what they say. A line starting with `#` is a comment and
unknown keys are ignored.

| Key | Required | Meaning |
| :--- | :--- | :--- |
| `path` | yes | Absolute path to the target executable. |
| `arg` | no, repeatable | Argument inserted before the caller's own, exactly as written. |
| `workdir` | no | Working directory for the child. Without it the child starts in **the target's** directory, not the caller's. |
| `env` | no, repeatable | `NAME=value` for the child. For parameters, not secrets. |
| `envref` | no, repeatable | `NAME=reference`; the value is read when the shim runs, so it never sits in the file. |

```ini
# shimbox config
path=C:\Tools\ffmpeg\bin\ffmpeg.exe
arg=-hide_banner
workdir=C:\Tools\ffmpeg
env=FFMPEG_DEBUG=1
envref=GITHUB_TOKEN=cred:shimbox/github
```

An `envref` points at one of two places:

| Reference | Reads |
| :--- | :--- |
| `file:<path>` | The whole file, without its trailing newlines. |
| `cred:<target>` | A generic credential from the Windows Credential Manager. |

The manager stores secrets as `shimbox/<name>` credentials and offers only names
it wrote itself; one stored value can be referenced by any number of shims, each
calling it whatever its target expects. A credential is a DPAPI blob your own
session can decrypt, so this keeps values **out of the file**, not out of reach
of anything running as you. Unresolvable references fail loudly instead of
starting the child without a value it wanted.

## Repository layout

```text
crates\runner           the shim runner, both variants from one source
apps\gui\src-tauri      the manager's Rust commands (scan, generate, secrets)
apps\gui\ui             its frontend: plain HTML, CSS and ES modules
docs\techspec.md        the specification every change is made against
scripts                 build, test and smoke entry points
bin                     staged artifacts (git-ignored)
```

The UI has no build step and no npm dependencies; it is embedded into
`shimbox.exe` at compile time, so a UI change needs a rebuild to show up. Every
string goes through a `data-i18n` key with one dictionary per language, so a
language is one file in `apps\gui\ui\locales` plus one line in `i18n.js` — the
two registered today are English and Russian.

## Notes

- **Unsigned**: the shape is a small unsigned executable that calls
  `CreateProcess`, copied under many different names, which is a realistic
  SmartScreen or Defender false positive. Signing is the mitigation that
  actually works; see `docs/techspec.md` 6.
- **No `.bat` / `.cmd` / `.ps1` targets** in v1; the runner launches `.exe`
  files directly.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).

---

## 👨‍💻 Author & Support

Developed with ❤️ by [Sergey Goloveshko](https://sergey.is-a.dev) — **AI & Systems Engineer**.

- **Portfolio**: [sergey.is-a.dev](https://sergey.is-a.dev)
- **Telegram Support**: [@itz2bot](https://t.me/itz2bot?start=shimbox_page_readme)
- **Report an Issue**: [GitHub Issues](https://github.com/goloveshko/shimbox/issues)
