//! `morningstar-preview <app>`: the supervisor. See the crate's
//! documentation and `--help`.

use std::process::ExitCode;

use morningstar_preview::options::{Options, USAGE};
use morningstar_preview::supervisor;

fn main() -> ExitCode {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("preview: {error}");
            return ExitCode::from(2);
        }
    };
    if options.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if options.app.is_empty() {
        eprintln!("preview: which app? Give its Cargo package, such as barua-talk.\n");
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    supervisor::run(&options)
}
