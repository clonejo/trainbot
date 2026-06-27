use camino::{Utf8Path, Utf8PathBuf};

const DB_FILE: &str = "db.sqlite3";
const BLOBS_DIR: &str = "blobs";

#[derive(Debug, Clone)]
pub struct DataStore {
    pub data_dir: Utf8PathBuf,
}

impl DataStore {
    pub fn new(data_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn db_path(&self) -> Utf8PathBuf {
        self.data_dir.join(DB_FILE)
    }

    pub fn blob_path(&self, name: impl AsRef<Utf8Path>) -> Utf8PathBuf {
        self.data_dir.join(BLOBS_DIR).join(name)
    }

    pub fn blob_thumb_path(&self, name: &str) -> Utf8PathBuf {
        self.blob_path(thumb_name(name))
    }
}

/// Returns the thumbnail filename for a given blob filename.
/// `"pic.jpg"` → `"pic.thumb.jpg"`
pub fn thumb_name(blob_name: &str) -> String {
    match blob_name.rfind('.') {
        Some(dot) => {
            let (stem, ext) = blob_name.split_at(dot);
            format!("{}.thumb{}", stem, ext)
        }
        None => format!("{}.thumb", blob_name),
    }
}

/// Inverts `thumb_name`: `"pic.thumb.jpg"` → `"pic.jpg"`, `"blob.thumb"` → `"blob"`.
///
/// Mirrors Go's `RevertThumbName`: strip the last extension, check if the remainder has
/// its own extension (".thumb" suffix); if not, the stripped extension was the thumb suffix.
pub fn revert_thumb_name(name: &str) -> String {
    // last_ext: ".jpg" for "pic.thumb.jpg", ".thumb" for "blob.thumb"
    let last_dot = match name.rfind('.') {
        Some(i) => i,
        None => return name.to_string(),
    };
    let without_last_ext = &name[..last_dot]; // "pic.thumb" or "blob"
    let last_ext = &name[last_dot..]; // ".jpg" or ".thumb"

    // second_ext: ".thumb" for "pic.thumb", "" for "blob"
    match without_last_ext.rfind('.') {
        Some(dot2) => {
            // "pic.thumb" → stem="pic", second_ext=".thumb"
            let second_ext = &without_last_ext[dot2..];
            if second_ext == ".thumb" {
                format!("{}{}", &without_last_ext[..dot2], last_ext)
            } else {
                name.to_string()
            }
        }
        None => {
            // No second dot → the last extension was the thumb suffix → strip it.
            without_last_ext.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datastore_paths() {
        let ds = DataStore::new("data");
        assert_eq!(ds.db_path(), Utf8Path::new("data/db.sqlite3"));
        assert_eq!(
            ds.blob_path("testblob"),
            Utf8Path::new("data/blobs/testblob")
        );
        assert_eq!(
            ds.blob_thumb_path("testblob.jpg"),
            Utf8Path::new("data/blobs/testblob.thumb.jpg")
        );
    }

    #[test]
    fn thumb_names() {
        assert_eq!(thumb_name("pic.jpg"), "pic.thumb.jpg");
        assert_eq!(thumb_name("blob"), "blob.thumb");
        assert_eq!(revert_thumb_name("pic.thumb.jpg"), "pic.jpg");
        assert_eq!(revert_thumb_name("blob.thumb"), "blob");
    }
}
