use ::clap::Parser;

/// Helper for developing rofi-unicode
#[derive(Parser)]
enum Args {
    Generate(generate::Args),
}

mod generate;

#[allow(clippy::redundant_closure)]
fn main() -> anyhow::Result<()> {
    match Args::parse() {
        Args::Generate(args) => generate::generate(args),
    }
}
