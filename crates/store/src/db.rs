use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../../internal/pkg/db/schema.sql");

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Open (or create) the SQLite database at `path`, run the embedded schema, and
/// return the connection.  WAL mode and foreign-key enforcement are applied.
pub fn open(path: impl AsRef<camino::Utf8Path>) -> Result<Connection> {
    let conn = Connection::open(path.as_ref())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_schema() {
        let dir = tempdir();
        let conn = open(dir.join("test.db")).unwrap();
        // Verify trains_v2 exists and has the expected columns.
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('trains_v2') ORDER BY cid")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            cols,
            [
                "id",
                "start_ts",
                "n_frames",
                "length_px",
                "speed_px_s",
                "accel_px_s_2",
                "px_per_m",
                "uploaded",
                "cleaned_up"
            ]
        );
    }

    #[test]
    fn open_twice_idempotent() {
        let dir = tempdir();
        let path = dir.join("test.db");
        open(&path).unwrap();
        open(&path).unwrap();
    }

    #[test]
    fn schema_diff_empty() {
        // Verify that the temperatures and trains_v2 tables from the schema exist.
        let conn = open(tempdir().join("test.db")).unwrap();
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert!(tables.contains(&"trains_v2".to_string()));
        assert!(tables.contains(&"temperatures".to_string()));
    }

    fn tempdir() -> camino::Utf8PathBuf {
        let dir = camino::Utf8PathBuf::try_from(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "store_test_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
