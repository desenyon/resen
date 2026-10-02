use crate::{config::Paths, domain::ResearchRun};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};

pub struct Store {
    connection: Connection,
    _lock: std::fs::File,
}
impl Store {
    pub fn open(paths: &Paths) -> Result<Self> {
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.root.join("session.lock"))?;
        lock.try_lock().context("This research directory is already open in another Resen process. Close it or use a different --data-dir.")?;
        let connection = Connection::open(paths.root.join("research.db"))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, body TEXT NOT NULL);
            PRAGMA user_version=1;")?;
        Ok(Self {
            connection,
            _lock: lock,
        })
    }
    pub fn save(&self, run: &ResearchRun) -> Result<()> {
        self.connection.execute("INSERT INTO runs(id,created_at,body) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET body=excluded.body", params![run.id, run.created_at.to_rfc3339(), serde_json::to_string(run)?])?;
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<ResearchRun>> {
        let mut query = self
            .connection
            .prepare("SELECT body FROM runs ORDER BY created_at DESC LIMIT 200")?;
        let rows = query.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|row| serde_json::from_str(&row?).context("Could not read a stored research run"))
            .collect()
    }
    pub fn get(&self, id: &str) -> Result<ResearchRun> {
        let body: String = self
            .connection
            .query_row("SELECT body FROM runs WHERE id=?1", [id], |row| row.get(0))
            .context("Run not found")?;
        Ok(serde_json::from_str(&body)?)
    }
    pub fn recover_interrupted(&self) -> Result<usize> {
        let mut count = 0;
        for mut run in self.list()? {
            if run.status == "running" {
                run.status = "interrupted".into();
                run.warnings.push("The application stopped before this run completed. Partial work was preserved.".into());
                self.save(&run)?;
                count += 1;
            }
        }
        Ok(count)
    }
}
