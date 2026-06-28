use trainbot_core::FitMethod;

fn parse_args() -> FitMethod {
    for arg in std::env::args().skip(1) {
        if let Some(val) = arg.strip_prefix("--fit-method=") {
            match val {
                "ols" => return FitMethod::Ols,
                "ransac" => return FitMethod::Ransac,
                other => {
                    eprintln!("trainbot: unknown --fit-method value: {other:?} (expected ols|ransac)");
                    std::process::exit(2);
                }
            }
        }
    }
    FitMethod::Ols
}

fn main() {
    let _fit_method = parse_args();
    eprintln!("trainbot: not yet implemented (Phase 0 stub)");
    std::process::exit(1);
}
