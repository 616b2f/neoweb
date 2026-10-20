use rusqlite::{Connection, Result, params};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub uri: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visited: String,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let path = match db_path {
            Some(p) => p,
            None => {
                let dirs = directories::ProjectDirs::from("org", "neoweb", "neoweb");
                let data_dir = dirs
                    .map(|d| d.data_local_dir().to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                std::fs::create_dir_all(&data_dir).ok();
                data_dir.join("neoweb.db")
            }
        };

        info!("Opening database at {:?}", path);
        let conn = Connection::open(path)?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uri TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1,
                last_visited DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_history_uri ON history(uri);
            CREATE INDEX IF NOT EXISTS idx_history_last_visited ON history(last_visited);
            ",
        )?;

        Ok(Self { conn })
    }

    pub fn record_history(&mut self, uri: &str, title: &str) -> Result<()> {
        if uri.is_empty() || uri.starts_with("about:") {
            return Ok(());
        }

        self.conn.execute(
            "
            INSERT INTO history (uri, title, visit_count, last_visited)
            VALUES (?1, ?2, 1, CURRENT_TIMESTAMP)
            ON CONFLICT(uri) DO UPDATE SET
                title = excluded.title,
                visit_count = history.visit_count + 1,
                last_visited = CURRENT_TIMESTAMP;
            ",
            params![uri, title],
        )?;

        Ok(())
    }

    pub fn search_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "
            SELECT id, uri, title, visit_count, last_visited
            FROM history
            WHERE uri LIKE ?1 OR title LIKE ?1
            ORDER BY last_visited DESC
            LIMIT ?2
            ",
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                uri: row.get(1)?,
                title: row.get(2)?,
                visit_count: row.get(3)?,
                last_visited: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uri TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                visit_count INTEGER NOT NULL DEFAULT 1,
                last_visited DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            ",
        )?;
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_history() {
        let mut storage = Storage::in_memory().unwrap();

        storage
            .record_history("https://servo.org", "Servo Engine")
            .unwrap();
        storage
            .record_history("https://servo.org", "Servo Engine")
            .unwrap(); // increments count

        let history = storage.search_history("servo", 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].uri, "https://servo.org");
        assert_eq!(history[0].visit_count, 2);
    }
}
