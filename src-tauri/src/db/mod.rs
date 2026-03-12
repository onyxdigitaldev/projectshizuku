use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub id: i64,
    pub anime_id: i64,
    pub title: String,
    pub episode: String,
    pub barrel: String,
    pub series_title: String,
    pub file_path: Option<String>,
    pub status: String,
    pub progress: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MokurokuEntry {
    pub anime_id: i64,
    pub title: String,
    pub cover_image: Option<String>,
    pub added_at: String,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("shizuku.db");
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS watch_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                episode TEXT NOT NULL,
                watched_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(anime_id, episode)
            );

            CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                episode TEXT NOT NULL,
                barrel TEXT NOT NULL DEFAULT '',
                series_title TEXT NOT NULL DEFAULT '',
                file_path TEXT,
                status TEXT NOT NULL DEFAULT 'queued',
                progress REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(anime_id, episode)
            );

            CREATE TABLE IF NOT EXISTS mokuroku (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                anime_id INTEGER NOT NULL UNIQUE,
                title TEXT NOT NULL,
                cover_image TEXT,
                added_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS cache (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                expires_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            INSERT OR IGNORE INTO settings (key, value) VALUES ('allow_adult', 'false');
            INSERT OR IGNORE INTO settings (key, value) VALUES ('download_dir', '~/Shizuku');
            ",
        )?;
        // Migrations for existing databases
        conn.execute_batch("ALTER TABLE downloads ADD COLUMN barrel TEXT NOT NULL DEFAULT '';").ok();
        conn.execute_batch("ALTER TABLE downloads ADD COLUMN series_title TEXT NOT NULL DEFAULT '';").ok();
        Ok(())
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let result = stmt
            .query_row(rusqlite::params![key], |row| row.get::<_, String>(0))
            .ok();
        Ok(result)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    // --- Cache ---

    pub fn get_cache(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .prepare("SELECT value FROM cache WHERE key = ?1 AND expires_at > datetime('now')")?
            .query_row(rusqlite::params![key], |row| row.get::<_, String>(0))
            .ok();
        Ok(result)
    }

    pub fn set_cache(&self, key: &str, value: &str, ttl_seconds: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO cache (key, value, expires_at) VALUES (?1, ?2, datetime('now', '+' || ?3 || ' seconds'))",
            rusqlite::params![key, value, ttl_seconds],
        )?;
        // Opportunistic cleanup
        conn.execute("DELETE FROM cache WHERE expires_at <= datetime('now')", []).ok();
        Ok(())
    }

    // --- Watch History ---

    pub fn add_to_history(&self, anime_id: i64, title: &str, episode: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO watch_history (anime_id, title, episode, watched_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params![anime_id, title, episode],
        )?;
        Ok(())
    }

    pub fn get_history(&self, limit: i32) -> Result<Vec<(i64, String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT anime_id, title, episode, watched_at FROM watch_history
             ORDER BY watched_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_continue_watching(&self, limit: i32) -> Result<Vec<(i64, String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w.anime_id, w.title, w.episode, w.watched_at
             FROM watch_history w
             INNER JOIN (SELECT anime_id, MAX(watched_at) AS max_at FROM watch_history GROUP BY anime_id) latest
             ON w.anime_id = latest.anime_id AND w.watched_at = latest.max_at
             ORDER BY w.watched_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_watched_episodes(&self, anime_id: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT episode FROM watch_history WHERE anime_id = ?1")?;
        let rows = stmt
            .query_map(rusqlite::params![anime_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // --- Downloads ---

    /// Returns Some(download_id) if queued, None if already complete or in-progress
    pub fn queue_download(
        &self,
        anime_id: i64,
        title: &str,
        episode: &str,
        barrel: &str,
        series_title: &str,
    ) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<(i64, String, Option<String>)> = conn
            .prepare("SELECT id, status, file_path FROM downloads WHERE anime_id = ?1 AND episode = ?2")?
            .query_row(rusqlite::params![anime_id, episode], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .ok();

        if let Some((id, status, file_path)) = existing {
            // Skip if currently downloading or queued
            if status == "downloading" || status == "queued" {
                return Ok(None);
            }
            // Skip if complete and file exists with non-zero size
            if status == "complete" {
                if let Some(ref fp) = file_path {
                    let path = std::path::Path::new(fp);
                    if path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                        return Ok(None);
                    }
                }
            }
            // Failed, or complete with missing/empty file — reset and retry
            conn.execute(
                "UPDATE downloads SET title = ?1, barrel = ?2, series_title = ?3, status = 'queued', progress = 0.0, file_path = NULL WHERE id = ?4",
                rusqlite::params![title, barrel, series_title, id],
            )?;
            return Ok(Some(id));
        }

        conn.execute(
            "INSERT INTO downloads (anime_id, title, episode, barrel, series_title, status, progress)
             VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0.0)",
            rusqlite::params![anime_id, title, episode, barrel, series_title],
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    pub fn get_downloads(&self) -> Result<Vec<DownloadEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, anime_id, title, episode, barrel, series_title, file_path, status, progress
             FROM downloads ORDER BY series_title, barrel, CAST(episode AS REAL), episode",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(DownloadEntry {
                    id: row.get(0)?,
                    anime_id: row.get(1)?,
                    title: row.get(2)?,
                    episode: row.get(3)?,
                    barrel: row.get(4)?,
                    series_title: row.get(5)?,
                    file_path: row.get(6)?,
                    status: row.get(7)?,
                    progress: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn update_download_progress(&self, id: i64, progress: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET progress = ?1, status = 'downloading' WHERE id = ?2",
            rusqlite::params![progress, id],
        )?;
        Ok(())
    }

    pub fn update_download_complete(&self, id: i64, file_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'complete', progress = 1.0, file_path = ?1 WHERE id = ?2",
            rusqlite::params![file_path, id],
        )?;
        Ok(())
    }

    pub fn update_download_failed(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET status = 'failed' WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    /// Reset all failed downloads to 'queued' for retry
    pub fn retry_failed_downloads(&self) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE downloads SET status = 'queued', progress = 0.0, file_path = NULL WHERE status = 'failed'",
            [],
        )?;
        Ok(count as i32)
    }

    /// Delete a download entry
    pub fn delete_download(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    /// Clear all completed downloads from the list
    pub fn clear_completed_downloads(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE status = 'complete'", [])?;
        Ok(())
    }

    pub fn get_download_path(&self, anime_id: i64, episode: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT file_path FROM downloads WHERE anime_id = ?1 AND episode = ?2 AND status = 'complete'",
        )?;
        let result = stmt
            .query_row(rusqlite::params![anime_id, episode], |row| {
                row.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten();
        Ok(result)
    }

    // --- Mokuroku (Watchlist) ---

    pub fn add_to_mokuroku(&self, anime_id: i64, title: &str, cover_image: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO mokuroku (anime_id, title, cover_image, added_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params![anime_id, title, cover_image],
        )?;
        Ok(())
    }

    pub fn remove_from_mokuroku(&self, anime_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM mokuroku WHERE anime_id = ?1",
            rusqlite::params![anime_id],
        )?;
        Ok(())
    }

    pub fn get_mokuroku(&self) -> Result<Vec<MokurokuEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT anime_id, title, cover_image, added_at FROM mokuroku ORDER BY added_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MokurokuEntry {
                    anime_id: row.get(0)?,
                    title: row.get(1)?,
                    cover_image: row.get(2)?,
                    added_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn is_in_mokuroku(&self, anime_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM mokuroku WHERE anime_id = ?1")?;
        let count: i32 = stmt.query_row(rusqlite::params![anime_id], |row| row.get(0))?;
        Ok(count > 0)
    }
}
