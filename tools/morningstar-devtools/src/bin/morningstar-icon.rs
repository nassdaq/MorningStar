//! `morningstar-icon`: see [`morningstar_devtools::commands::icon`] and `--help`.
//! The same command is `morningstar icon` in the `morningstar-cli` crate.

fn main() -> std::process::ExitCode {
    morningstar_devtools::commands::icon::main(std::env::args().skip(1))
}
