use clap::Parser;

fn main() {
    let cli = codesql::cli::Cli::parse();
    if let Err(error) = codesql::run(cli) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
