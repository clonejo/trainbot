mod args;
mod cleanup;
mod confighelper;
mod detect;

fn basename_no_arch(argv0: &str) -> &str {
    let base = argv0.rsplit('/').next().unwrap_or(argv0);
    // Strip arch suffixes like -x86_64, -aarch64, -arm64
    if let Some(pos) = base.rfind('-') {
        let suffix = &base[pos + 1..];
        if matches!(suffix, "x86_64" | "aarch64" | "arm64" | "amd64") {
            return &base[..pos];
        }
    }
    base
}

fn main() {
    let mut argv: Vec<String> = std::env::args().collect();
    let base = basename_no_arch(argv.first().map(|s| s.as_str()).unwrap_or("trainbot"));

    match base {
        "confighelper" => confighelper::run(argv),
        "cleanup" => cleanup::run(argv),
        _ => {
            // Check if first positional arg is a known subcommand name
            match argv.get(1).map(|s| s.as_str()) {
                Some("confighelper") => {
                    argv.remove(1);
                    confighelper::run(argv);
                }
                Some("cleanup") => {
                    argv.remove(1);
                    cleanup::run(argv);
                }
                Some("detect") => {
                    argv.remove(1);
                    detect::run(argv);
                }
                _ => detect::run(argv),
            }
        }
    }
}
