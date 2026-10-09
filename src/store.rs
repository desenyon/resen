use crate::{config::Paths, domain::ResearchRun};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::Serialize;

pub const SCHEMA_VERSION: i64 = 2;
pub const ARCHIVE_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug)]
pub struct ArchiveQuery {
    pub search: String,
    pub status: Option<String>,
    pub limit: usize,
    pub offset: usize,
}
impl Default for ArchiveQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            status: None,
            limit: ARCHIVE_PAGE_SIZE,
            offset: 0,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ArchivePage {
    pub runs: Vec<ResearchRun>,
    pub total: usize,
    pub offset: usize,
    pub unreadable: usize,
}
#[derive(Debug, Serialize)]
pub struct ArchiveIssue {
    pub id: String,
    pub reason: String,
}

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
        let mut connection = Connection::open(paths.root.join("research.db"))?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            (0..=SCHEMA_VERSION).contains(&version),
            "Archive schema {version} is newer than this Resen supports ({SCHEMA_VERSION}); use a newer Resen. The database was not migrated."
        );
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        if version < SCHEMA_VERSION {
            let transaction = connection.transaction()?;
            if version == 0 {
                transaction.execute_batch("CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, body TEXT NOT NULL);")?;
            }
            transaction.execute_batch(
                "ALTER TABLE runs ADD COLUMN status TEXT NOT NULL DEFAULT '';
                ALTER TABLE runs ADD COLUMN search_text TEXT NOT NULL DEFAULT '';
                ALTER TABLE runs ADD COLUMN read_error TEXT;
                CREATE INDEX runs_archive_order ON runs(created_at DESC, id DESC);
                CREATE INDEX runs_status_order ON runs(status, created_at DESC, id DESC);",
            )?;
            inspect_records(&transaction, false)?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction
                .commit()
                .context("Could not migrate research archive")?;
        }
        Ok(Self {
            connection,
            _lock: lock,
        })
    }
    pub fn save(&self, run: &ResearchRun) -> Result<()> {
        save(&self.connection, run)
    }
    /// Compatibility convenience: the latest 200 healthy records. Use query for full history.
    pub fn list(&self) -> Result<Vec<ResearchRun>> {
        Ok(self
            .query(&ArchiveQuery {
                limit: 200,
                ..Default::default()
            })?
            .runs)
    }
    pub fn query(&self, query: &ArchiveQuery) -> Result<ArchivePage> {
        ensure!(
            (1..=1000).contains(&query.limit),
            "Archive limit must be between 1 and 1000"
        );
        let offset = i64::try_from(query.offset).context("Archive offset is too large")?;
        let search = query.search.to_lowercase();
        // If a selected record was damaged after startup, quarantine it and refill the page.
        // Raw JSON stays in runs, and the offset always counts healthy matching records.
        loop {
            let mut statement = self.connection.prepare(
                "SELECT id, body FROM runs
                WHERE read_error IS NULL AND instr(search_text, ?1) > 0
                AND (?2 IS NULL OR status = ?2)
                ORDER BY created_at DESC, id DESC LIMIT ?3 OFFSET ?4",
            )?;
            let rows = statement.query_map(
                params![search, query.status, query.limit as i64, offset],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            let mut runs = Vec::new();
            let mut issues = Vec::new();
            for row in rows {
                let (id, body) = row?;
                match decode(&id, &body) {
                    Ok(run) => runs.push(run),
                    Err(error) => issues.push((id, error.to_string())),
                }
            }
            drop(statement);
            if issues.is_empty() {
                let total: i64 = self.connection.query_row(
                    "SELECT count(*) FROM runs
                    WHERE read_error IS NULL AND instr(search_text, ?1) > 0
                    AND (?2 IS NULL OR status = ?2)",
                    params![search, query.status],
                    |row| row.get(0),
                )?;
                return Ok(ArchivePage {
                    runs,
                    total: usize::try_from(total)?,
                    offset: query.offset,
                    unreadable: self.unreadable_count()?,
                });
            }
            for (id, error) in issues {
                self.connection.execute(
                    "UPDATE runs SET read_error=?2 WHERE id=?1",
                    params![id, error],
                )?;
            }
        }
    }
    pub fn unreadable_count(&self) -> Result<usize> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM runs WHERE read_error IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(usize::try_from(count)?)
    }
    pub fn issues(&self) -> Result<Vec<ArchiveIssue>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, read_error FROM runs WHERE read_error IS NOT NULL ORDER BY id")?;
        Ok(statement
            .query_map([], |row| {
                Ok(ArchiveIssue {
                    id: row.get(0)?,
                    reason: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn get(&self, id: &str) -> Result<ResearchRun> {
        let body: String = self
            .connection
            .query_row("SELECT body FROM runs WHERE id=?1", [id], |row| row.get(0))
            .with_context(|| format!("Run {id} not found"))?;
        decode(id, &body)
    }
    /// A full, bounded-memory integrity/recovery pass; never limited by archive pagination.
    pub fn recover_interrupted(&self) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let count = inspect_records(&transaction, true)?;
        transaction
            .commit()
            .context("Could not commit interrupted research recovery")?;
        Ok(count)
    }
}
fn decode(id: &str, body: &str) -> Result<ResearchRun> {
    let run: ResearchRun = serde_json::from_str(body).with_context(|| {
        format!("Stored run {id} is unreadable; original JSON preserved in research.db")
    })?;
    ensure!(
        run.id == id,
        "Stored run {id} has a mismatched JSON ID; original JSON preserved in research.db"
    );
    Ok(run)
}
fn search_text(run: &ResearchRun) -> String {
    format!(
        "{} {} {} {} {} {} {}",
        run.id,
        run.request.question,
        run.request.symbols.join(" "),
        run.request.kind.label(),
        run.provider,
        run.model,
        run.status
    )
    .to_lowercase()
}
fn created_at(run: &ResearchRun) -> String {
    run.created_at
        .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}
fn save(connection: &Connection, run: &ResearchRun) -> Result<()> {
    connection.execute(
        "INSERT INTO runs(id,created_at,body,status,search_text) VALUES(?1,?2,?3,?4,?5)
        ON CONFLICT(id) DO UPDATE SET created_at=excluded.created_at, body=excluded.body,
        status=excluded.status, search_text=excluded.search_text, read_error=NULL",
        params![
            run.id,
            created_at(run),
            serde_json::to_string(run)?,
            run.status,
            search_text(run)
        ],
    )?;
    Ok(())
}
fn inspect_records(connection: &Connection, recover: bool) -> Result<usize> {
    // Updating non-key columns while scanning in primary-key order keeps each row visited once.
    let mut statement = connection.prepare(
        "SELECT id, body, created_at, status, search_text, read_error FROM runs ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let body: String = row.get(1)?;
        match decode(&id, &body) {
            Ok(mut run) => {
                if recover && run.status == "running" {
                    run.status = "interrupted".into();
                    run.warnings.push("The application stopped before this run completed. Partial work was preserved.".into());
                    save(connection, &run)?;
                    count += 1;
                } else if row.get::<_, String>(2)? != created_at(&run)
                    || row.get::<_, String>(3)? != run.status
                    || row.get::<_, String>(4)? != search_text(&run)
                    || row.get::<_, Option<String>>(5)?.is_some()
                {
                    connection.execute("UPDATE runs SET created_at=?2,status=?3,search_text=?4,read_error=NULL WHERE id=?1",
                        params![id, created_at(&run), run.status, search_text(&run)])?;
                }
            }
            Err(error) => {
                connection.execute(
                    "UPDATE runs SET read_error=?2 WHERE id=?1",
                    params![id, error.to_string()],
                )?;
            }
        }
    }
    Ok(count)
}
