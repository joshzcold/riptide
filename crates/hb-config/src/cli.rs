//! Command line options. Chromium switches (anything else starting with `-`)
//! are left for CEF, which reads the same command line.

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub basedir: Option<PathBuf>,
    pub print_paths: bool,
    pub lua_types: bool,
    /// Where URLs open: tab, tab-bg, window or current.
    pub target: Option<String>,
    pub help: bool,
    pub version: bool,
    pub urls: Vec<String>,
}

pub const USAGE: &str = "\
Usage: hackers-browser [OPTIONS] [URL...]

Options:
  --basedir DIR   Keep config in DIR/config and browser data in DIR/data
  --paths         Print the config and data directories, then exit
  --lua-types     Print lua-language-server definitions for config.lua
  --target WHERE  Open URLs in a running browser as: tab, tab-bg, window, current

Arguments starting with ':' run as commands, e.g. hackers-browser ':open -t x'.
If the browser is already running for this profile, the arguments go to it.
  -h, --help      Show this help
  -V, --version   Show the version

Other --switches are passed to Chromium.";

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut cli = Cli::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--basedir" => {
                    let dir = args.next().ok_or("--basedir needs a directory")?;
                    cli.basedir = Some(PathBuf::from(dir));
                }
                "--paths" => cli.print_paths = true,
                "--lua-types" => cli.lua_types = true,
                "--target" => {
                    let target = args
                        .next()
                        .ok_or("--target needs tab, tab-bg, window or current")?;
                    if !matches!(target.as_str(), "tab" | "tab-bg" | "window" | "current") {
                        return Err(format!("--target: unknown target {target:?}"));
                    }
                    cli.target = Some(target);
                }
                "-h" | "--help" => cli.help = true,
                "-V" | "--version" => cli.version = true,
                _ => match arg.strip_prefix("--basedir=") {
                    Some(dir) => cli.basedir = Some(PathBuf::from(dir)),
                    None if arg.starts_with('-') => {}
                    None => cli.urls.push(arg),
                },
            }
        }
        Ok(cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, String> {
        Cli::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn urls_and_options() {
        let cli = parse(&[
            "--basedir",
            "/b",
            "example.com",
            "--enable-logging",
            "rust docs",
        ])
        .unwrap();
        assert_eq!(cli.basedir, Some(PathBuf::from("/b")));
        assert_eq!(cli.urls, ["example.com", "rust docs"]);
        assert_eq!(
            parse(&["--basedir=/x"]).unwrap().basedir,
            Some(PathBuf::from("/x"))
        );
        assert!(parse(&["--paths"]).unwrap().print_paths);
        assert!(parse(&["--basedir"]).is_err());
    }
}
