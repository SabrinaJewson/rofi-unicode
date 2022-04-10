use ::{
    anyhow::Context as _,
    clap::Parser,
    std::{
        env,
        ffi::OsStr,
        fs,
        os::unix::ffi::OsStrExt,
        path::{Path, PathBuf},
    },
};

mod unicode;

/// Helper for developing and installing rofi-unicode
#[derive(Parser)]
enum Args {
    Generate(GenerateArgs),
    Install(InstallArgs),
}

/// Generate all the system-wide default configuration files and put them in `./etc`.
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

/// Install `rofi-unicode` to the system.
/// Often must be run as root.
/// Requires both the library to be built and the default configuration files to be generated.
#[derive(Parser)]
pub struct InstallArgs {
    /// The prefix Rofi was installed at.
    #[clap(long, env, default_value = "/")]
    rofi_prefix: PathBuf,

    /// The Cargo profile to install from.
    #[clap(long, default_value = "release")]
    profile: String,
}

#[allow(clippy::redundant_closure)]
fn main() -> anyhow::Result<()> {
    let mut workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    workspace_root.pop();
    env::set_current_dir(workspace_root).context("failed to set current dir")?;

    match Args::parse() {
        Args::Generate(args) => generate(args),
        Args::Install(args) => install(args),
    }
}

fn generate(args: GenerateArgs) -> anyhow::Result<()> {
    unicode::generate(args.ucd, "etc/unicode".as_ref())?;
    Ok(())
}

fn install(
    InstallArgs {
        rofi_prefix,
        profile,
    }: InstallArgs,
) -> anyhow::Result<()> {
    let profile = if profile == "dev" {
        // The `dev` profile uses `target/debug` for historical reasons
        "debug".into()
    } else {
        profile
    };

    let so_source = PathBuf::from_iter(["target", &*profile, "librofi_unicode.so"]);
    let mut so_dest = rofi_prefix;
    so_dest.extend(["lib", "rofi", "unicode.so"]);
    fs::copy(&*so_source, &*so_dest).with_context(|| {
        format!(
            "failed to copy {} to {}",
            so_source.display(),
            so_dest.display()
        )
    })?;

    let config_dirs = env::var_os("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    let config_dir = config_dirs
        .as_bytes()
        .split(|&byte| byte == b':')
        .next()
        .unwrap();
    let etc = Path::new(OsStr::from_bytes(config_dir)).join("rofi-unicode");
    dircpy::CopyBuilder::new("etc", &*etc)
        .overwrite(true)
        .run()
        .with_context(|| format!("failed to copy etc to {}", etc.display()))?;

    Ok(())
}
