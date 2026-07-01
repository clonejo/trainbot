use chrono::{DateTime, FixedOffset};
use rusqlite::{Connection, Result as SqlResult, Row};

use crate::ts::{format_db_ts, format_file_ts, parse_db_ts};

/// Represents a minimal train row (id + start_ts).
#[derive(Debug, Clone, PartialEq)]
pub struct Train {
    pub id: i64,
    pub start_ts: DateTime<FixedOffset>,
}

impl Train {
    pub fn img_file_name(&self) -> String {
        self.file_name("jpg")
    }

    pub fn gif_file_name(&self) -> String {
        self.file_name("gif")
    }

    pub fn file_name(&self, extension: &str) -> String {
        format!("train_{}.{}", format_file_ts(&self.start_ts), extension)
    }
}

fn parse_train(row: &Row<'_>) -> SqlResult<Train> {
    let id: i64 = row.get(0)?;
    let ts_str: String = row.get(1)?;
    let start_ts = parse_db_ts(&ts_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Train { id, start_ts })
}

/// Insert a new train sighting.  Returns the new row id.
pub fn insert_train(
    conn: &Connection,
    start_ts: &DateTime<FixedOffset>,
    n_frames: i64,
    length_px: f64,
    speed_px_s: f64,
    accel_px_s2: f64,
    px_per_m: f64,
) -> SqlResult<i64> {
    conn.query_row(
        "INSERT INTO trains_v2 (start_ts, n_frames, length_px, speed_px_s, accel_px_s_2, px_per_m)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
        rusqlite::params![
            format_db_ts(start_ts),
            n_frames,
            length_px,
            speed_px_s,
            accel_px_s2,
            px_per_m,
        ],
        |r| r.get(0),
    )
}

/// Return the oldest not-yet-uploaded train, or `None` if there is none.
pub fn get_next_upload(conn: &Connection) -> SqlResult<Option<Train>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, start_ts FROM trains_v2 WHERE NOT uploaded ORDER BY id ASC LIMIT 1",
    )?;
    let mut rows = stmt.query([])?;
    rows.next()?.map(parse_train).transpose()
}

/// Mark a train as uploaded.  Errors if the row was already uploaded.
pub fn set_uploaded(conn: &Connection, id: i64) -> SqlResult<()> {
    let n = conn.execute(
        "UPDATE trains_v2 SET uploaded = TRUE WHERE id = ?1 AND NOT uploaded",
        [id],
    )?;
    if n != 1 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

/// Return the oldest uploaded-but-not-cleaned-up train that is older than the 100 most recent
/// blobs, or `None` if there is none.
pub fn get_next_cleanup(conn: &Connection) -> SqlResult<Option<Train>> {
    const KEEP_LAST_N: i64 = 100;
    let mut stmt = conn.prepare_cached(
        "SELECT id, start_ts FROM trains_v2
         WHERE uploaded AND NOT cleaned_up
         ORDER BY id DESC LIMIT 1 OFFSET ?1",
    )?;
    let mut rows = stmt.query([KEEP_LAST_N - 1])?;
    rows.next()?.map(parse_train).transpose()
}

/// Mark a train's blobs as cleaned up locally.  Errors if already cleaned up.
pub fn set_cleaned_up(conn: &Connection, id: i64) -> SqlResult<()> {
    let n = conn.execute(
        "UPDATE trains_v2 SET cleaned_up = TRUE WHERE id = ?1 AND NOT cleaned_up",
        [id],
    )?;
    if n != 1 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

/// Return a set of all known blob filenames (image + gif, no thumbnails).
pub fn get_all_blobs(conn: &Connection) -> SqlResult<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare_cached("SELECT id, start_ts FROM trains_v2")?;
    let mut out = std::collections::HashSet::new();
    let rows = stmt.query_map([], parse_train)?;
    for row in rows {
        let t = row?;
        out.insert(t.img_file_name());
        out.insert(t.gif_file_name());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open;
    use chrono::DateTime;

    fn parse(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn temp_conn() -> Connection {
        let dir = camino::Utf8PathBuf::try_from(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "store_q_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
        std::fs::create_dir_all(&dir).unwrap();
        open(dir.join("test.db")).unwrap()
    }

    #[test]
    fn file_names_match_go() {
        let t = Train {
            id: 1,
            start_ts: parse("2023-12-24T09:58:52.660009478Z"),
        };
        assert_eq!(t.img_file_name(), "train_20231224_095852.66_Z.jpg");
        assert_eq!(t.gif_file_name(), "train_20231224_095852.66_Z.gif");

        let t2 = Train {
            id: 2,
            start_ts: parse("2023-12-24T11:19:12.839262415Z"),
        };
        assert_eq!(t2.img_file_name(), "train_20231224_111912.839_Z.jpg");

        let t3 = Train {
            id: 3,
            start_ts: parse("2023-10-28T17:31:50.709434526+01:00"),
        };
        assert_eq!(t3.img_file_name(), "train_20231028_173150.709_+01:00.jpg");

        let t4 = Train {
            id: 4,
            start_ts: parse("2023-11-25T15:49:46.958831882+00:00"),
        };
        assert_eq!(t4.img_file_name(), "train_20231125_154946.958_Z.jpg");

        let t5 = Train {
            id: 5,
            start_ts: parse("2023-03-28T06:32:16.516941205+01:00"),
        };
        assert_eq!(t5.img_file_name(), "train_20230328_063216.516_+01:00.jpg");
    }

    #[test]
    fn insert_and_upload_flow() {
        let conn = temp_conn();
        let t0 = parse("2023-06-10T16:20:58.805+02:00");
        let id = insert_train(&conn, &t0, 10, 100.0, 21.5, 0.3, 8.0).unwrap();
        assert!(id > 0);

        // Fetch for upload.
        let up = get_next_upload(&conn).unwrap().unwrap();
        assert_eq!(up.id, id);
        assert_eq!(up.start_ts, t0);

        // Mark uploaded; second call should fail.
        set_uploaded(&conn, id).unwrap();
        assert!(set_uploaded(&conn, id).is_err());

        // No more to upload.
        assert!(get_next_upload(&conn).unwrap().is_none());

        // Cleanup query: none (only 1 row, keep_last_n=100).
        assert!(get_next_cleanup(&conn).unwrap().is_none());

        set_cleaned_up(&conn, id).unwrap();
        assert!(set_cleaned_up(&conn, id).is_err());
    }

    #[test]
    fn cleanup_offset_logic() {
        let conn = temp_conn();
        let base = parse("2023-06-10T16:20:58.805+02:00");

        // Insert 101 rows, mark all uploaded.
        let mut ids = Vec::new();
        for i in 0..101i64 {
            let ts = base + chrono::Duration::seconds(i);
            let id = insert_train(&conn, &ts, 1, 1.0, 1.0, 0.0, 1.0).unwrap();
            set_uploaded(&conn, id).unwrap();
            ids.push(id);
        }

        // Now there are 101 uploaded rows; offset=99 → 2 eligible rows.
        let c = get_next_cleanup(&conn).unwrap();
        assert!(c.is_some());
        set_cleaned_up(&conn, c.unwrap().id).unwrap();

        let c2 = get_next_cleanup(&conn).unwrap();
        assert!(c2.is_some());

        // After cleaning the 2nd, none remain beyond keep_last_n.
        set_cleaned_up(&conn, c2.unwrap().id).unwrap();
        assert!(get_next_cleanup(&conn).unwrap().is_none());
    }

    #[test]
    fn get_all_blobs() {
        let conn = temp_conn();
        let t0 = parse("2023-06-10T16:20:58.805+02:00");
        let t1 = parse("2023-06-10T16:21:05.982+02:00");
        insert_train(&conn, &t0, 1, 1.0, 1.0, 0.0, 1.0).unwrap();
        insert_train(&conn, &t1, 1, 1.0, 1.0, 0.0, 1.0).unwrap();

        let blobs = super::get_all_blobs(&conn).unwrap();
        assert_eq!(blobs.len(), 4);
        assert!(blobs.contains("train_20230610_162058.805_+02:00.jpg"));
        assert!(blobs.contains("train_20230610_162058.805_+02:00.gif"));
    }

    #[test]
    fn db_timestamp_serialization() {
        let conn = temp_conn();
        let t0 = parse("2023-06-10T16:20:58.805+02:00");
        insert_train(&conn, &t0, 1, 1.0, 1.0, 0.0, 1.0).unwrap();

        let raw: String = conn
            .query_row(
                "SELECT start_ts FROM trains_v2 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raw, "2023-06-10T16:20:58.805+02:00");
    }
}
