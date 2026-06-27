use camino::Utf8PathBuf;
use std::process::Command;

fn main() {
    let go_bin = std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(Utf8PathBuf::from)
        .expect("usage: conformance <go-trainbot-binary>");

    let status = Command::new(&go_bin)
        .arg("--help")
        .status()
        .unwrap_or_else(|e| panic!("failed to run {:?}: {e}", go_bin));

    // go-arg exits 0 on --help
    assert!(
        status.success(),
        "Go oracle {:?} exited with {status}",
        go_bin
    );

    println!(
        "Phase 0 conformance OK: Go oracle at {:?} is runnable",
        go_bin
    );
}
