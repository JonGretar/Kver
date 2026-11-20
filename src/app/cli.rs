//! CLI syntax and requests shared by the launcher and the running application.

use std::{ffi::OsString, path::PathBuf};

use crate::settings::AppPaths;

use super::instance::{self, Instance};
use super::remote_pdf;
use super::windows::{OpenMode, OpenRequest, OpenTarget};

const HELP: &str = "\
Usage: kver [--new-window | --reuse-window] [--] [PDF path or HTTP(S) URL]

Open a PDF in Kver, forwarding to the running app when available.
Relative paths are resolved from this terminal's working directory.

  --new-window    Open a new reader window
  --reuse-window  Replace the PDF in the most recently active reader window
  --help, -h      Show this help
  --version, -V   Show the version
  --              Treat the following argument as a path, even if it starts with -

By default, files reuse the most recently active reader window.
The [application].open_files preference applies to all file-open commands.
With no path, activate the most recent reader, or open an empty window.";

enum Invocation {
    Open(OpenRequest),
    Help,
    Version,
}

/// Run the CLI, including single-instance handoff before initializing GPUI.
pub fn run() -> Result<(), String> {
    let request = match parse(std::env::args_os().skip(1))? {
        Invocation::Help => {
            println!("{HELP}");
            return Ok(());
        }
        Invocation::Version => {
            println!("kver {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Invocation::Open(request) => request,
    };
    let paths = AppPaths::resolve().ok_or("Cannot locate Kver's user data directory")?;
    match instance::connect_or_listen(paths.local_data())
        .map_err(|error| format!("Cannot connect to or start Kver: {error}"))?
    {
        Instance::Secondary(stream) => instance::forward(stream, &request)
            .map_err(|error| format!("Kver did not accept the request: {error}")),
        Instance::Primary(primary) => super::run_primary(request, *primary),
    }
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut mode = OpenMode::Default;
    let mut path = None;
    let mut literal = false;
    for arg in args {
        if !literal {
            match arg.to_str() {
                Some("--") => {
                    literal = true;
                    continue;
                }
                Some("--help" | "-h") => return Ok(Invocation::Help),
                Some("--version" | "-V") => return Ok(Invocation::Version),
                Some("--new-window" | "--reuse-window") => {
                    let requested = if arg == "--new-window" {
                        OpenMode::New
                    } else {
                        OpenMode::Reuse
                    };
                    if mode != OpenMode::Default && mode != requested {
                        return Err("Use either --new-window or --reuse-window, not both".into());
                    }
                    mode = requested;
                    continue;
                }
                _ if arg.as_encoded_bytes().starts_with(b"-") => {
                    return Err(format!("Unknown option {arg:?}. Use kver --help."));
                }
                _ => {}
            }
        }
        if path.replace((arg, literal)).is_some() {
            return Err("Expected at most one PDF path or URL. Use kver --help.".into());
        }
    }
    let target = path
        .map(|(arg, literal)| {
            if !literal
                && let Some(value) = arg.to_str()
                && value.split_once("://").is_some_and(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                })
            {
                return remote_pdf::parse_url(value).map(OpenTarget::Url);
            }
            std::path::absolute(PathBuf::from(arg))
                .map(OpenTarget::File)
                .map_err(|error| format!("Cannot resolve the PDF path: {error}"))
        })
        .transpose()?;
    Ok(Invocation::Open(OpenRequest { target, mode }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(args: &[&str]) -> OpenRequest {
        match parse(args.iter().map(OsString::from)).unwrap() {
            Invocation::Open(request) => request,
            _ => panic!("expected an open request"),
        }
    }

    #[test]
    fn paths_are_absolute_and_preserve_special_characters() {
        let request = request(&["space # % æ.pdf"]);
        assert_eq!(
            request.target,
            Some(OpenTarget::File(
                std::env::current_dir().unwrap().join("space # % æ.pdf")
            ))
        );
        assert_eq!(request.mode, OpenMode::Default);
    }

    #[test]
    fn explicit_window_modes_and_literal_paths() {
        assert_eq!(request(&["--new-window"]).mode, OpenMode::New);
        assert_eq!(request(&["--reuse-window", "a.pdf"]).mode, OpenMode::Reuse);
        assert!(request(&[]).target.is_none());
        assert!(matches!(
            request(&["--", "-book.pdf"]).target,
            Some(OpenTarget::File(path)) if path.ends_with("-book.pdf")
        ));
        assert!(matches!(
            request(&["HTTPS://example.org/download?id=42"]).target,
            Some(OpenTarget::Url(url)) if url.as_str() == "https://example.org/download?id=42"
        ));
        assert!(parse(["https://"].map(OsString::from)).is_err());
        assert!(parse(["--new-window", "--reuse-window"].map(OsString::from)).is_err());
        assert!(parse(["--bogus"].map(OsString::from)).is_err());
        assert!(parse(["a.pdf", "b.pdf"].map(OsString::from)).is_err());
    }

    #[test]
    fn informational_options_do_not_launch_the_app() {
        assert!(matches!(
            parse(["--help".into()]).unwrap(),
            Invocation::Help
        ));
        assert!(matches!(parse(["-V".into()]).unwrap(), Invocation::Version));
    }
}
