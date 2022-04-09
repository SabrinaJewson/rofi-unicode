use ::std::env;

fn help() -> ! {
    eprintln!("Helper for developing rofi-unicode");
    eprintln!();
    eprintln!("SUBCOMMANDS:");
    eprintln!("    generate     Generate src/generated.rs based on the UCD");
    std::process::exit(1)
}

mod generate;

#[allow(clippy::redundant_closure)]
fn main() -> anyhow::Result<()> {
    match &*env::args().nth(1).unwrap_or_else(|| help()) {
        "generate" => generate::generate(),
        _ => help(),
    }
}
