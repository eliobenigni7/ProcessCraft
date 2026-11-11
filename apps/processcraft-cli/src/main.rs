mod args;
mod mcp;
mod run;
use clap::Parser;
fn main() {
    let args = args::Args::parse();
    if let Err(error) = run::execute(args.command) {
        eprintln!("{}", error.message);
        std::process::exit(error.code);
    }
}
