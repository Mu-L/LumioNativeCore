//! Thin launcher for the single Cargo-metadata based repository validator.
use std::path::Path;
use std::process::{Command, ExitCode};
fn main() -> ExitCode {
    let argument = std::env::args().nth(1).unwrap_or_default();
    let mode = match argument.as_str() {
        "check-dep-dag" => "dag",
        "assert-no-native-artifacts" => "artifacts",
        _ => {
            eprintln!("usage: cargo xtask <check-dep-dag|assert-no-native-artifacts>");
            return ExitCode::from(2);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    for interpreter in ["python3", "python"] {
        match Command::new(interpreter)
            .arg(root.join("tools/check_repository.py"))
            .arg(mode)
            .current_dir(root)
            .status()
        {
            Ok(status) => return ExitCode::from(status.code().unwrap_or(1) as u8),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!("validator failed: {error}");
                return ExitCode::from(2);
            }
        }
    }
    eprintln!("Python 3 is required for repository tooling");
    ExitCode::from(2)
}
