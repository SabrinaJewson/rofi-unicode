use ::{clap::Parser, std::path::PathBuf};

/// Helper for developing rofi-unicode
#[derive(Parser)]
enum Args {
    GenerateUnicode(generate_unicode::Args),
}

mod generate_unicode;

#[allow(clippy::redundant_closure)]
fn main() -> anyhow::Result<()> {
    let mut out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out_dir.pop();
    out_dir.push("default_lists");

    match Args::parse() {
        Args::GenerateUnicode(args) => {
            out_dir.push("unicode");
            generate_unicode::generate(args, &*out_dir)
        }
    }
}
