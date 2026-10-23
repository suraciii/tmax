use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut command = clap::Command::new("tmax")
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"));
    command.get_matches_mut();
    command.print_help()?;
    io::stdout().write_all(b"\n")
}
