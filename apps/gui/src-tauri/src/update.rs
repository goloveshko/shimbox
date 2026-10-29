//! Whether a newer release of shimbox is out (docs/techspec.md 5.6).
//!
//! The asking is the frontend's own - the answer is a public file and the window
//! already is a browser, so the one thing this module keeps is *when the
//! question was last asked*. That moment lives in `$APPCONFIG/.update.json`
//! beside the theme and the pickers' folders, and it is written whether or not
//! the ask answered: a machine with no network is not served by asking again on
//! every start. The body the same answer carries is read here rather than in the
//! frontend for the reason the sentence above gives: a card shows plain text and
//! a body is markdown, and that translation is worth a test.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state;

const FILE: &str = ".update.json";

/// Long enough that a day of restarts asks once, short enough that a release
/// does not sit unnoticed for a week.
const DAY: u64 = 60 * 60 * 24;

/// The moment of the last ask. A file that cannot be read is a never-asked,
/// which is what the window should do about it.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Asked {
    at: u64,
}

/// The release the frontend read. `version` is the tag as GitHub carries it, so
/// a leading `v` is expected here rather than stripped on the way in.
#[derive(serde::Deserialize)]
pub struct Release {
    version: String,
    url: String,
    notes: String,
}

/// What a card is made of: the same three, once the running version has been
/// told it is behind. `notes` has already been read by [`plain_text`], since the
/// card shows one paragraph of text and a release body is markdown.
#[derive(serde::Serialize)]
pub struct Available {
    version: String,
    url: String,
    notes: String,
}

/// The markup GitHub would have rendered, taken off a release body.
///
/// What survives is the shape of the writing: a heading keeps its line and loses
/// its `#`, a fenced block keeps its lines and loses its fences, a link keeps its
/// words and loses its address — which the card already offers as a button. A
/// line of a list is kept as written, because `- ` reads as a bullet in any
/// medium. Single `*` and `_` are left alone: a lone star is as likely to be a
/// bullet as an emphasis, and an underscore is half the names a config holds.
fn plain_text(markdown: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut fenced = false;

    for line in markdown.lines() {
        let text = line.trim_start();
        // Read from the raw line, before any markup comes off it.
        let indented = line.len() > text.len();
        if text.starts_with("```") || text.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            kept.push(text.to_string());
            continue;
        }
        if text.is_empty() {
            kept.push(String::new());
            continue;
        }
        if is_rule(text) {
            continue;
        }
        let text = heading(text);
        let text = text.strip_prefix("> ").unwrap_or(text);
        let text = unwrap(&links(text), "**");
        let text = unwrap(&text, "`").trim_end().to_string();
        // A line the author indented belongs to the one above it: markdown makes
        // that a soft break, and a card that printed it as a hard one would show a
        // wrapped sentence as three.
        if indented && kept.last().is_some_and(|last| !last.is_empty()) {
            let last = kept.last_mut().expect("the line the check above read");
            last.push(' ');
            last.push_str(&text);
            continue;
        }
        kept.push(text);
    }

    // The card has one paragraph, so the blank lines that separate blocks in
    // markdown are worth one line each and no more.
    let mut text = kept.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_string()
}

/// A line of nothing but three or more repeats of one punctuation is a rule, and
/// a card that shows it shows a line of dashes nobody wrote.
fn is_rule(text: &str) -> bool {
    let first = match text.chars().next() {
        Some(character) => character,
        None => return false,
    };
    matches!(first, '-' | '*' | '_')
        && text.chars().count() >= 3
        && text.chars().all(|character| character == first)
}

/// The words of a `#` heading, however many levels it was written with.
fn heading(text: &str) -> &str {
    if !text.starts_with('#') {
        return text;
    }
    let words = text.trim_start_matches('#');
    words.strip_prefix(' ').unwrap_or(words)
}

/// `[words](address)` and `![words](address)` keep their words. The address is
/// the one thing a reader cannot use inside the card, and the button beside it
/// opens the page that holds every link the body has.
fn links(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        // The `!` of an image sits outside the brackets, so it is already in
        // `out` and comes back out again here.
        if out.ends_with('!') {
            out.pop();
        }
        let words = &rest[open + 1..];
        let Some(close) = words.find(']') else {
            out.push_str(&rest[open..]);
            return out;
        };
        out.push_str(&words[..close]);
        rest = &words[close + 1..];
        // The address is what follows the words in parentheses, and it is the part
        // a card cannot use. A `(` with no `)` after it is not one, and is kept.
        rest = rest
            .strip_prefix('(')
            .and_then(|address| address.find(')').map(|end| &address[end + 1..]))
            .unwrap_or(rest);
    }
    out.push_str(rest);
    out
}

/// The text between two markers, with the markers gone. An unclosed one is left
/// as written: a body with a stray `**` is the author's, not a thing to guess at.
fn unwrap(text: &str, marker: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(marker) {
        let after = &rest[start + marker.len()..];
        match after.find(marker) {
            Some(end) if end > 0 => {
                out.push_str(&rest[..start]);
                out.push_str(&after[..end]);
                rest = &after[end + marker.len()..];
            }
            _ => {
                out.push_str(rest);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The numbers a version is made of. A leading `v` is how a tag is named, and
/// what follows a `-` or a `+` is a label rather than a number, so `v1.2.3-rc.1`
/// reads as 1.2.3 - which is never above 1.2.3 in the comparison below, and
/// that is the wanted answer for a pre-release.
fn numbers(version: &str) -> Vec<u64> {
    let core = version.trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or_default();
    core.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// Only a bigger number is a newer release. Equal, smaller and unreadable all
/// answer no, so a tag that is not a version never brings a card.
fn is_newer(candidate: &str, running: &str) -> bool {
    let (candidate, running) = (numbers(candidate), numbers(running));
    for index in 0..candidate.len().max(running.len()) {
        let (a, b) = (
            candidate.get(index).copied().unwrap_or(0),
            running.get(index).copied().unwrap_or(0),
        );
        if a != b {
            return a > b;
        }
    }
    false
}

fn due_at(asked: &Asked, now: u64) -> bool {
    now.saturating_sub(asked.at) >= DAY
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

// The rule in `state`, bound to this file's name and its shape: the one moment
// the question was last asked, which is unreadable in exactly the way a never
// asked is.
fn read(file: &Path) -> Asked {
    state::read(file)
}

fn write(file: &Path, asked: Asked) {
    state::write(file, &asked);
}

fn state_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    state::file(app, FILE)
}

/// Whether the frontend should ask GitHub now.
#[tauri::command]
pub fn update_due(app: tauri::AppHandle) -> bool {
    state_file(&app).is_some_and(|file| due_at(&read(&file), now()))
}

/// Records the ask, whatever it found, and answers whether it is worth a card.
/// The address comes off the network, so it goes to `open_link`, which only ever
/// hands a browser an address on the two hosts that card already names.
#[tauri::command]
pub fn update_seen(app: tauri::AppHandle, release: Option<Release>) -> Option<Available> {
    let file = state_file(&app)?;
    write(&file, Asked { at: now() });
    let release = release?;
    is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(Available {
        version: release.version,
        url: release.url,
        notes: plain_text(&release.notes),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        crate::testutil::scratch("update", name)
    }

    #[test]
    fn a_day_of_restarts_asks_once() {
        let root = scratch("gate");
        let file = root.join(FILE);

        assert!(due_at(&read(&file), 10 * DAY), "never asked is due");

        write(&file, Asked { at: 10 * DAY });
        assert!(!due_at(&read(&file), 10 * DAY + 60), "asked a minute ago");
        assert!(
            !due_at(&read(&file), 11 * DAY - 1),
            "asked just under a day ago"
        );
        assert!(due_at(&read(&file), 11 * DAY), "a day is a day");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_that_is_not_an_answer_reads_as_no_answer() {
        let root = scratch("garbage");
        let file = root.join(FILE);

        fs::write(&file, "not json").unwrap();
        assert_eq!(read(&file).at, 0);

        fs::write(&file, r#"{"at":"yesterday"}"#).unwrap();
        assert_eq!(read(&file).at, 0);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_body_arrives_as_the_text_a_card_shows() {
        // Shaped like the bodies this project actually publishes: a download line
        // of bold links, headings, one fenced sample, a list whose second line is
        // a continuation, and a rule between the two languages.
        let body = concat!(
            "**Download: [shimbox.zip](https://example/a.zip)** · [sha256](https://example/a)\n",
            "\n",
            "## What changed\n",
            "\n",
            "A shim no longer starts its target from `C:\\Tools\\ffmpeg`:\n",
            "\n",
            "```text\n",
            "magick @list.txt output.pdf\n",
            "```\n",
            "\n",
            "- relative paths are read from the caller\n",
            "  and so is a relative `workdir`\n",
            "\n",
            "---\n",
            "\n",
            "## Что изменилось\n",
            "\n",
            "**Обновите** шимы этой машины.\n",
        );
        assert_eq!(
            plain_text(body),
            concat!(
                "Download: shimbox.zip · sha256\n",
                "\n",
                "What changed\n",
                "\n",
                "A shim no longer starts its target from C:\\Tools\\ffmpeg:\n",
                "\n",
                "magick @list.txt output.pdf\n",
                "\n",
                "- relative paths are read from the caller and so is a relative workdir\n",
                "\n",
                "Что изменилось\n",
                "\n",
                "Обновите шимы этой машины.",
            )
        );
    }

    #[test]
    fn markup_that_does_not_close_is_left_as_the_author_wrote_it() {
        // A guess would rewrite the note rather than show it, and a card is not
        // where an unbalanced marker is worth fixing.
        assert_eq!(plain_text("a ** b"), "a ** b");
        assert_eq!(plain_text("a ` b"), "a ` b");
        assert_eq!(plain_text("[no address"), "[no address");
        // An unclosed fence still holds the lines it opened on, which is the only
        // reading that does not drop what the author wrote.
        assert_eq!(plain_text("```\nfenced\n"), "fenced");
        assert_eq!(plain_text("  \n\n\n\n"), "");
        assert_eq!(plain_text("#Title"), "Title");
    }

    #[test]
    fn an_indented_line_joins_the_one_it_continues() {
        assert_eq!(plain_text("- one\n  two"), "- one two");
        // Across a blank line it is a paragraph of its own, however it is indented.
        assert_eq!(plain_text("- one\n\n  two"), "- one\n\ntwo");
        // Inside a fence the author's own breaks are the content.
        assert_eq!(plain_text("```\n  a\n  b\n```"), "a\nb");
    }

    #[test]
    fn only_a_bigger_number_is_a_newer_release() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1.0", "1.0.0"), "a tag's v is not part of it");
        assert!(is_newer("1.10.0", "1.9.0"), "1.10 is not 1.1");
        assert!(!is_newer("1.0", "1.0.0"), "a shorter name says the same");
        assert!(is_newer("2.0.0", "1.99.99"));

        assert!(!is_newer("1.0.0", "1.0.0"), "this build is not an update");
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"), "a pre-release is not it");
        assert!(!is_newer("", "1.0.0"), "no answer is no update");
        assert!(!is_newer("nightly", "1.0.0"), "a name is not a number");
    }
}
