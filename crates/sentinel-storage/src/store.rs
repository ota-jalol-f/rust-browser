//! The encrypted [`Store`] and its row types.
//!
//! All URL/title values cross the SQLite boundary as ciphertext; decryption
//! happens only in the getters here, holding plaintext in memory for the shortest
//! span needed to build the returned structs.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use sentinel_core::{Error, Result};
use sentinel_crypto::{decrypt, encrypt, MasterKey};

/// One visited-page record from the history table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Stable row id.
    pub id: i64,
    /// The visited URL (decrypted).
    pub url: String,
    /// The page title at visit time (decrypted).
    pub title: String,
    /// Unix timestamp (seconds) of the visit.
    pub visited_at: i64,
}

/// One saved bookmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    /// Stable row id.
    pub id: i64,
    /// The bookmarked URL (decrypted).
    pub url: String,
    /// The bookmark title (decrypted).
    pub title: String,
    /// Unix timestamp (seconds) the bookmark was created.
    pub created_at: i64,
}

/// An encrypted browser data store backed by SQLite.
///
/// Open with [`Store::open`] (file-backed) or [`Store::open_in_memory`] (tests).
/// The [`MasterKey`] is held for the lifetime of the store and used to encrypt on
/// write and decrypt on read.
pub struct Store {
    conn: Connection,
    key: MasterKey,
}

impl Store {
    /// Open (or create) a file-backed store at `path`, ensuring the schema exists.
    ///
    /// # Errors
    /// [`Error::Storage`] if the database cannot be opened or the schema migration
    /// fails.
    pub fn open(path: impl AsRef<Path>, key: MasterKey) -> Result<Self> {
        let conn = Connection::open(path).map_err(map_db)?;
        Self::from_conn(conn, key)
    }

    /// Open an in-memory store (no file), primarily for tests.
    ///
    /// # Errors
    /// [`Error::Storage`] if the schema migration fails.
    pub fn open_in_memory(key: MasterKey) -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(map_db)?;
        Self::from_conn(conn, key)
    }

    /// Shared constructor: install the schema, then wrap.
    fn from_conn(conn: Connection, key: MasterKey) -> Result<Self> {
        conn.execute_batch(SCHEMA).map_err(map_db)?;
        Ok(Self { conn, key })
    }

    /// Record a visit to `url` with `title`, stamped with the current time.
    /// Returns the new row id.
    ///
    /// Both `url` and `title` are encrypted before they touch the database.
    ///
    /// # Errors
    /// [`Error::Crypto`] on encryption failure, [`Error::Storage`] on insert failure.
    pub fn add_history(&self, url: &str, title: &str) -> Result<i64> {
        let url_c = encrypt(&self.key, url.as_bytes())?;
        let title_c = encrypt(&self.key, title.as_bytes())?;
        self.conn
            .execute(
                "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![url_c, title_c, now_unix()],
            )
            .map_err(map_db)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return up to `limit` most-recent history entries, newest first.
    ///
    /// # Errors
    /// [`Error::Storage`] on query failure, [`Error::Crypto`] if a stored row
    /// cannot be decrypted (e.g. opened with the wrong key).
    pub fn recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, title, visited_at FROM history ORDER BY id DESC LIMIT ?1")
            .map_err(map_db)?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(map_db)?;

        let mut out = Vec::new();
        for row in rows {
            let (id, url_c, title_c, visited_at) = row.map_err(map_db)?;
            out.push(HistoryEntry {
                id,
                url: self.decrypt_str(&url_c)?,
                title: self.decrypt_str(&title_c)?,
                visited_at,
            });
        }
        Ok(out)
    }

    /// Save a bookmark for `url`/`title`, stamped with the current time.
    /// Returns the new row id.
    ///
    /// # Errors
    /// [`Error::Crypto`] on encryption failure, [`Error::Storage`] on insert failure.
    pub fn add_bookmark(&self, url: &str, title: &str) -> Result<i64> {
        let url_c = encrypt(&self.key, url.as_bytes())?;
        let title_c = encrypt(&self.key, title.as_bytes())?;
        self.conn
            .execute(
                "INSERT INTO bookmarks (url, title, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![url_c, title_c, now_unix()],
            )
            .map_err(map_db)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List all bookmarks, newest first.
    ///
    /// # Errors
    /// [`Error::Storage`] on query failure, [`Error::Crypto`] on decrypt failure.
    pub fn list_bookmarks(&self) -> Result<Vec<Bookmark>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, title, created_at FROM bookmarks ORDER BY id DESC")
            .map_err(map_db)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(map_db)?;

        let mut out = Vec::new();
        for row in rows {
            let (id, url_c, title_c, created_at) = row.map_err(map_db)?;
            out.push(Bookmark {
                id,
                url: self.decrypt_str(&url_c)?,
                title: self.decrypt_str(&title_c)?,
                created_at,
            });
        }
        Ok(out)
    }

    /// Remove the bookmark with `id`. Returns `true` if a row was deleted.
    ///
    /// # Errors
    /// [`Error::Storage`] on delete failure.
    pub fn remove_bookmark(&self, id: i64) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM bookmarks WHERE id = ?1", rusqlite::params![id])
            .map_err(map_db)?;
        Ok(n > 0)
    }

    /// Decrypt a ciphertext column into a UTF-8 string.
    fn decrypt_str(&self, ciphertext: &[u8]) -> Result<String> {
        let bytes = decrypt(&self.key, ciphertext)?;
        String::from_utf8(bytes).map_err(|e| Error::Storage(format!("invalid utf-8 in store: {e}")))
    }
}

/// Schema applied on open. `IF NOT EXISTS` makes it an idempotent migration.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url BLOB NOT NULL,
    title BLOB NOT NULL,
    visited_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS bookmarks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url BLOB NOT NULL,
    title BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
";

/// Map a `rusqlite` error into the crate error type.
fn map_db(e: rusqlite::Error) -> Error {
    Error::Storage(e.to_string())
}

/// Current Unix time in whole seconds. Falls back to 0 if the clock predates the
/// epoch (not physically possible on a sane system; avoids an `unwrap`).
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_crypto::SALT_LEN;

    fn key(seed: u8) -> MasterKey {
        MasterKey::derive("pw", &[seed; SALT_LEN]).unwrap()
    }

    #[test]
    fn history_roundtrips_decrypted() {
        let store = Store::open_in_memory(key(1)).unwrap();
        store.add_history("https://a.com", "Site A").unwrap();
        let recent = store.recent_history(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].url, "https://a.com");
        assert_eq!(recent[0].title, "Site A");
        assert!(recent[0].visited_at > 0);
    }

    #[test]
    fn recent_history_is_newest_first_and_limited() {
        let store = Store::open_in_memory(key(1)).unwrap();
        store.add_history("https://1.com", "1").unwrap();
        store.add_history("https://2.com", "2").unwrap();
        store.add_history("https://3.com", "3").unwrap();
        let recent = store.recent_history(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].url, "https://3.com");
        assert_eq!(recent[1].url, "https://2.com");
    }

    #[test]
    fn urls_are_ciphertext_at_rest() {
        // Prove the plaintext URL never appears in the stored blob.
        let store = Store::open_in_memory(key(1)).unwrap();
        store.add_history("https://secret.example", "T").unwrap();
        let raw: Vec<u8> = store
            .conn
            .query_row("SELECT url FROM history LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let needle = b"secret.example";
        assert!(
            !raw.windows(needle.len()).any(|w| w == needle),
            "plaintext URL leaked into the database blob"
        );
    }

    #[test]
    fn bookmarks_add_list_remove() {
        let store = Store::open_in_memory(key(1)).unwrap();
        let id = store.add_bookmark("https://b.com", "B").unwrap();
        let list = store.list_bookmarks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://b.com");
        assert!(store.remove_bookmark(id).unwrap());
        assert!(store.list_bookmarks().unwrap().is_empty());
        // Removing a non-existent id reports false rather than erroring.
        assert!(!store.remove_bookmark(9999).unwrap());
    }

    #[test]
    fn persists_across_reopen_with_correct_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sentinel.db");
        {
            let store = Store::open(&path, key(1)).unwrap();
            store.add_bookmark("https://persist.com", "P").unwrap();
        }
        let reopened = Store::open(&path, key(1)).unwrap();
        let list = reopened.list_bookmarks().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].url, "https://persist.com");
    }

    #[test]
    fn wrong_key_cannot_decrypt_existing_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sentinel.db");
        {
            let store = Store::open(&path, key(1)).unwrap();
            store.add_history("https://a.com", "A").unwrap();
        }
        // Re-open with a different key: the row exists but decryption must fail.
        let wrong = Store::open(&path, key(2)).unwrap();
        assert!(matches!(wrong.recent_history(10), Err(Error::Crypto(_))));
    }
}
