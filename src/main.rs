use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if args.next().is_some() {
        eprintln!("Usage: kver path/to/document.pdf");
        std::process::exit(2);
    }
    let path = first.map(PathBuf::from);

    kver::app::run(path);
}
