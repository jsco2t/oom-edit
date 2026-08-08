//! Command-line arguments for the `oom-edit` binary (FR-6.10).
//!
//! Hand-parsed (no `clap` — design rule: hand-roll small well-specified things).
//! Parsing runs in `main` **before** any terminal setup, so `--help`,
//! `--version`, and error paths print to a normal screen and never flash
//! the alternate screen.

use std::path::PathBuf;

/// The parsed CLI arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// Positional file path to open, if any.
    pub path: Option<PathBuf>,
    /// `--theme NAME`: theme name (T11 accepts only "default-dark").
    pub theme: Option<String>,
    /// Hidden debug flag: trigger a panic for NFR-5 verification.
    #[allow(dead_code)]
    pub panic_test: bool,
}

/// The result of parsing: either run with the given args, or print a message
/// (`--help`/`--version`) and exit 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome {
    /// Proceed to [`crate::run`] with these args.
    Run(Args),
    /// Print this message to stdout and exit 0 (`--help` / `--version`).
    Message(String),
}

impl Args {
    /// Parse arguments from an iterator (the first item — the program name — is
    /// skipped). Returns [`ParseOutcome::Message`] for `--help`/`--version`, or
    /// `Err(usage)` for an unknown flag or a missing value.
    ///
    /// # Errors
    ///
    /// Returns a usage string when a flag is unknown, a required value is
    /// missing, or an unexpected positional argument is supplied.
    pub fn parse<I: IntoIterator<Item = String>>(iter: I) -> Result<ParseOutcome, String> {
        let mut args = Args::default();
        let mut it = iter.into_iter().skip(1);
        let mut positional_count: u8 = 0;
        let mut positionals_only = false;

        while let Some(arg) = it.next() {
            if positionals_only {
                positional_count += 1;
                if positional_count > 1 {
                    return Err(usage(
                        "unexpected argument (only one positional path allowed)",
                    ));
                }
                args.path = Some(PathBuf::from(arg));
                continue;
            }

            // Split `--flag=value` into (`--flag`, Some("value")).
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) => (f.to_string(), Some(v.to_string())),
                None => (arg.clone(), None),
            };

            match flag.as_str() {
                "--help" | "-h" => return Ok(ParseOutcome::Message(help_text())),
                "--version" | "-V" => return Ok(ParseOutcome::Message(version_text())),
                "--theme" => {
                    args.theme = Some(take_value(&flag, inline, &mut it)?);
                }
                // Hidden debug flag: not documented in --help.
                "--panic-test" => {
                    reject_inline(&flag, inline.as_deref())?;
                    args.panic_test = true;
                }
                "--" => {
                    reject_inline(&flag, inline.as_deref())?;
                    positionals_only = true;
                }
                other if other.starts_with('-') => {
                    return Err(usage(&format!("unknown flag '{other}'")));
                }
                other => {
                    positional_count += 1;
                    if positional_count > 1 {
                        return Err(usage(
                            "unexpected argument (only one positional path allowed)",
                        ));
                    }
                    args.path = Some(PathBuf::from(other));
                }
            }
        }

        Ok(ParseOutcome::Run(args))
    }
}

/// A value-taking flag consumes its inline `=value` if present, else the next
/// argument. Errors if neither is available.
fn take_value<I: Iterator<Item = String>>(
    flag: &str,
    inline: Option<String>,
    it: &mut I,
) -> Result<String, String> {
    match inline {
        Some(v) => Ok(v),
        None => it
            .next()
            .ok_or_else(|| usage(&format!("flag '{flag}' requires a value"))),
    }
}

/// A boolean flag must not be given an `=value`.
fn reject_inline(flag: &str, inline: Option<&str>) -> Result<(), String> {
    if inline.is_some() {
        return Err(usage(&format!("flag '{flag}' takes no value")));
    }
    Ok(())
}

fn usage(problem: &str) -> String {
    format!("error: {problem}\n\n{}", help_text())
}

fn version_text() -> String {
    format!("oom-edit {}\n", env!("CARGO_PKG_VERSION"))
}

fn help_text() -> String {
    "\
oom-edit — a console/TUI markdown editor with Vim-style modal editing

USAGE:
    oom-edit [OPTIONS] [--] [path]

OPTIONS:
    --theme NAME        Theme name (built-in: default-dark)
    -h, --help          Print this help and exit
    -V, --version       Print version and exit

If no path is given, oom-edit opens a new buffer titled by the given path.
The file is created on the first save (:w).
"
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<ParseOutcome, String> {
        let mut v = vec!["oom-edit".to_string()];
        v.extend(args.iter().map(|s| (*s).to_string()));
        Args::parse(v)
    }

    fn run_args(args: &[&str]) -> Args {
        match parse(args).expect("parse ok") {
            ParseOutcome::Run(a) => a,
            ParseOutcome::Message(m) => panic!("expected Run, got message: {m}"),
        }
    }

    #[test]
    fn args_parse_no_args() {
        let a = run_args(&[]);
        assert!(a.path.is_none());
        assert!(a.theme.is_none());
        assert!(!a.panic_test);
    }

    #[test]
    fn args_parse_positional_path() {
        let a = run_args(&["/tmp/test.md"]);
        assert_eq!(a.path, Some(PathBuf::from("/tmp/test.md")));
    }

    #[test]
    fn args_parse_theme_flag_separate() {
        let a = run_args(&["--theme", "default-dark"]);
        assert_eq!(a.theme, Some("default-dark".to_string()));
    }

    #[test]
    fn args_parse_theme_flag_inline() {
        let a = run_args(&["--theme=dark"]);
        assert_eq!(a.theme, Some("dark".to_string()));
    }

    #[test]
    fn args_parse_all_together() {
        let a = run_args(&["/tmp/doc.md", "--theme", "default-dark"]);
        assert_eq!(a.path, Some(PathBuf::from("/tmp/doc.md")));
        assert_eq!(a.theme, Some("default-dark".to_string()));
    }

    #[test]
    fn help_is_message() {
        assert!(matches!(parse(&["--help"]), Ok(ParseOutcome::Message(_))));
        assert!(matches!(parse(&["-h"]), Ok(ParseOutcome::Message(_))));
    }

    #[test]
    fn version_is_message() {
        match parse(&["--version"]) {
            Ok(ParseOutcome::Message(m)) => assert!(m.contains(env!("CARGO_PKG_VERSION"))),
            other => panic!("expected version message, got {other:?}"),
        }
        match parse(&["-V"]) {
            Ok(ParseOutcome::Message(m)) => assert!(m.contains(env!("CARGO_PKG_VERSION"))),
            other => panic!("expected version message, got {other:?}"),
        }
    }

    #[test]
    fn unknown_flag_rejected() {
        let err = parse(&["--bogus"]).expect_err("unknown flag");
        assert!(err.contains("unknown flag '--bogus'"), "{err}");
        assert!(err.contains("USAGE:"), "usage text included: {err}");
    }

    #[test]
    fn missing_value_rejected() {
        let err = parse(&["--theme"]).expect_err("missing value");
        assert!(err.contains("requires a value"));
    }

    #[test]
    fn boolean_flag_takes_no_value() {
        let err = parse(&["--panic-test=yes"]).expect_err("bool takes no value");
        assert!(err.contains("takes no value"));
    }

    #[test]
    fn multiple_positional_rejected() {
        let err = parse(&["a.md", "b.md"]).expect_err("multiple positional");
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn double_dash_then_dash_file() {
        let a = run_args(&["--", "-notes.md"]);
        assert_eq!(a.path, Some(PathBuf::from("-notes.md")));
    }

    #[test]
    fn double_dash_then_normal_file() {
        let a = run_args(&["--", "file.md"]);
        assert_eq!(a.path, Some(PathBuf::from("file.md")));
    }

    #[test]
    fn double_dash_no_args_after() {
        let a = run_args(&["--"]);
        assert!(a.path.is_none());
    }

    #[test]
    fn double_dash_takes_no_inline_value() {
        let err = parse(&["--=notes.md"]).expect_err("separator takes no value");
        assert!(err.contains("takes no value"));
    }

    #[test]
    fn double_dash_multiple_positional_rejected() {
        let err = parse(&["--", "a.md", "b.md"]).expect_err("multiple positional");
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn double_dash_preserves_positional_count() {
        let err = parse(&["a.md", "--", "-b.md"]).expect_err("multiple positional");
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn dash_file_without_double_dash_rejected() {
        let err = parse(&["-notes.md"]).expect_err("dash file");
        assert!(err.contains("unknown flag"));
    }

    #[test]
    fn double_dash_then_help_is_a_filename() {
        let a = run_args(&["--", "--help"]);
        assert_eq!(a.path, Some(PathBuf::from("--help")));
    }

    #[test]
    fn help_text_contains_usage() {
        match parse(&["--help"]).unwrap() {
            ParseOutcome::Message(m) => {
                assert!(m.contains("USAGE:"));
                assert!(m.contains("oom-edit [OPTIONS] [--] [path]"));
                assert!(m.contains("oom-edit"));
            }
            other => panic!("expected help message, got {other:?}"),
        }
    }

    #[test]
    fn version_text_contains_package_version() {
        match parse(&["--version"]).unwrap() {
            ParseOutcome::Message(m) => {
                assert!(m.contains(env!("CARGO_PKG_VERSION")));
            }
            other => panic!("expected version message, got {other:?}"),
        }
    }
}
