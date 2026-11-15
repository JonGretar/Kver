fn main() {
    if let Err(error) = kver::app::cli::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
