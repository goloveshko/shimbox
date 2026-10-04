//! Parser for the flat `key=value` shim config (docs/techspec.md 3).

pub struct Config {
    pub path: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    /// Variables for the child, kept as written: the name is trimmed, the value
    /// is the rest of the line verbatim, so a value may contain `=` itself.
    pub env: Vec<(String, String)>,
}

pub fn parse(text: &str) -> Config {
    let mut config = Config {
        path: String::new(),
        args: Vec::new(),
        workdir: None,
        env: Vec::new(),
    };

    for line in text.lines() {
        let line = line.strip_prefix('\u{feff}').unwrap_or(line);
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // Values are taken verbatim: no quoting, no trimming, no escapes.
        match key.trim().to_ascii_lowercase().as_str() {
            "path" => config.path = value.to_string(),
            "arg" => config.args.push(value.to_string()),
            "workdir" => {
                if !value.is_empty() {
                    config.workdir = Some(value.to_string());
                }
            }
            "env" => {
                // `NAME=value` inside the value: only the first `=` separates
                // them, so a value may hold one. A line without it names no
                // variable and is dropped rather than guessed at.
                if let Some((name, value)) = value.split_once('=') {
                    let name = name.trim();
                    if !name.is_empty() {
                        config.env.push((name.to_string(), value.to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_every_known_key_in_order() {
        let config = parse("path=C:\\Tools\\a.exe\narg=-x\narg=-y \"z\"\nworkdir=C:\\Tools\n");
        assert_eq!(config.path, "C:\\Tools\\a.exe");
        assert_eq!(config.args, ["-x", "-y \"z\""]);
        assert_eq!(config.workdir.as_deref(), Some("C:\\Tools"));
    }

    #[test]
    fn ignores_comments_blanks_bom_and_unknown_keys() {
        let config = parse("\u{feff}# comment\n\n  \nfuture=1\npath=C:\\a.exe\r\n");
        assert_eq!(config.path, "C:\\a.exe");
        assert!(config.args.is_empty());
    }

    #[test]
    fn keeps_spaces_in_values() {
        let config = parse("path=C:\\My Tools\\a.exe \narg=--out my file.txt");
        assert_eq!(config.path, "C:\\My Tools\\a.exe ");
        assert_eq!(config.args, ["--out my file.txt"]);
    }

    #[test]
    fn empty_workdir_is_absent() {
        let config = parse("path=C:\\a.exe\nworkdir=\n");
        assert_eq!(config.workdir, None);
    }

    #[test]
    fn last_path_wins() {
        let config = parse("path=C:\\a.exe\npath=C:\\b.exe");
        assert_eq!(config.path, "C:\\b.exe");
    }

    #[test]
    fn reads_env_pairs_and_keeps_their_values() {
        let config = parse(
            "path=C:\\a.exe\nenv= NO_COLOR =1\nenv=URL=https://x/?a=b\nenv=EMPTY=\nenv=IGNORED\nenv=X=1\nenv=X=2\n",
        );
        assert_eq!(
            config.env,
            [
                ("NO_COLOR".to_string(), "1".to_string()),
                ("URL".to_string(), "https://x/?a=b".to_string()),
                // An empty value is a variable set to empty, which is not the
                // same as not setting it.
                ("EMPTY".to_string(), String::new()),
                // A repeated name is kept in order, so the last one wins when
                // the block is built.
                ("X".to_string(), "1".to_string()),
                ("X".to_string(), "2".to_string()),
            ]
        );
    }
}
