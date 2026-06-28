use clap::Parser;
use store::{DataStore, queries};
use trainbot_core::{LogConfig, init_logging};

use crate::args::CleanupArgs;

pub fn run(argv: Vec<String>) {
    let args = CleanupArgs::parse_from(argv);

    init_logging(&LogConfig {
        log_pretty: args.log_pretty,
        log_level: args.log_level.clone(),
    });

    let ds = DataStore::new(&args.data_dir);
    let blobs_dir = ds.data_dir.join("blobs");

    let conn = store::open(ds.db_path()).unwrap_or_else(|e| {
        eprintln!("error: cannot open DB: {e}");
        std::process::exit(1);
    });

    // Collect all known blob filenames (img + gif) and their thumbnails
    let db_blobs = match queries::get_all_blobs(&conn) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot query blobs: {e}");
            std::process::exit(1);
        }
    };

    let mut known: std::collections::HashSet<String> = db_blobs;
    let thumbs: Vec<String> = known
        .iter()
        .map(|n| store::datastore::thumb_name(n))
        .collect();
    for t in thumbs {
        known.insert(t);
    }

    // Walk blobs directory and find orphans
    let entries = match std::fs::read_dir(&blobs_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot read {blobs_dir}: {e}");
            std::process::exit(1);
        }
    };

    let mut missing = 0usize;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !known.contains(name_str.as_ref()) {
            println!("rm -f {}", entry.path().display());
            missing += 1;
        }
    }

    println!("# orphaned files: {missing}");
}
