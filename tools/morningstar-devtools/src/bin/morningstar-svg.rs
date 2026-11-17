//! `morningstar-svg`: see [`morningstar_devtools::commands::svg`] and `--help`.
//! The same command is `morningstar svg` in the `morningstar-cli` crate.

fn main() -> std::process::ExitCode {
    morningstar_devtools::commands::svg::main(std::env::args().skip(1))
}
