use ::{clap::Parser, std::path::PathBuf};

mod unicode;

/// Helper for developing and installing rofi-unicode
#[derive(Parser)]
enum Args {
    Generate(GenerateArgs),
}

/// Generate all the system-wide default configuration files and put them in `./etc`.
///
/// When installing, you should move all of these files into `/etc/xdg/rofi-unicode`.
#[derive(Parser)]
pub struct GenerateArgs {
    /// URL or fileystem path to the UCD.
    #[clap(
        long,
        default_value = "https://www.unicode.org/Public/UCD/latest/ucd/",
        env
    )]
    ucd: String,
}

#[allow(clippy::redundant_closure)]
fn main() -> anyhow::Result<()> {
    match Args::parse() {
        Args::Generate(args) => {
            let mut out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            out_dir.pop();
            out_dir.push("etc");

            unicode::generate(args.ucd, &*out_dir.join("unicode"))?;
        }
    }
    Ok(())
}
