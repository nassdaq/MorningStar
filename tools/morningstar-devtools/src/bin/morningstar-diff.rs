//! `morningstar-diff`: see [`morningstar_devtools::commands::diff`] and `--help`.
//! The same command is `morningstar diff` in the `morningstar-cli` crate.

fn main() -> std::process::ExitCode {
    morningstar_devtools::commands::diff::main(std::env::args().skip(1))
}
