//! Persistence layer — SQLite store + content-addressed blob storage.
//!
//! See TIA-STORE-001 through TIA-STORE-005.

use std::path::PathBuf;
use rusqlite::Connection;

use crate::engine::Origin;
use crate::change::ChangeSet;

/// The store holds the dependency graph, test history, and run payloads.
pub struct Store {
    conn: Connection,
    _db_path: PathBuf,
    _blob_dir: PathBuf,
}

/// Context loaded from the store for a single selection query.
pub struct SelectionContext {
    pub changed: Vec<u32>,
    pub unresolved: Vec<u32>,
    pub cu_deps: Vec<(u32, u32, Origin, u32)>,
    pub test_deps: Vec<(u32, u32, Origin, u32)>,
    pub always_run: Vec<u32>,
    pub comp_fallback: Vec<u32>,
    pub test_comp: Vec<(u32, u32)>,
}

impl Store {
    /// Open (or create) the store at the default location.
    pub fn open_default() -> miette::Result<Self> {
        let project_dir = find_project_root()?;
        Self::open(project_dir.join(".testaruda"))
    }

    /// Open (or create) the store at `path`.
    pub fn open(path: PathBuf) -> miette::Result<Self> {
        std::fs::create_dir_all(&path)
            .map_err(|e| miette::miette!("Failed to create store directory: {}", e))?;

        let blob_dir = path.join("blobs");
        std::fs::create_dir_all(&blob_dir)
            .map_err(|e| miette::miette!("Failed to create blob directory: {}", e))?;

        let db_path = path.join("store.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| miette::miette!("Failed to open store database: {}", e))?;

        Ok(Self { conn, _db_path: db_path, _blob_dir: blob_dir })
    }

    /// Initialize the store schema.
    pub fn initialize(&self) -> miette::Result<()> {
        self.conn.execute_batch("
            CREATE TABLE IF NOT EXISTS content_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                component TEXT NOT NULL,
                path TEXT NOT NULL,
                symbol TEXT,
                kind TEXT NOT NULL CHECK(kind IN ('source','config','fixture','lockfile','external')),
                fingerprint TEXT NOT NULL,
                UNIQUE(component, path, symbol)
            );
            CREATE TABLE IF NOT EXISTS test_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                component TEXT NOT NULL,
                adapter TEXT NOT NULL,
                node_id TEXT NOT NULL,
                UNIQUE(component, adapter, node_id)
            );
            CREATE TABLE IF NOT EXISTS dependency_edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                content_unit_id INTEGER NOT NULL REFERENCES content_units(id),
                environment TEXT NOT NULL DEFAULT '',
                origin TEXT NOT NULL CHECK(origin IN ('static','runtime','manual')),
                k_value INTEGER NOT NULL DEFAULT 1000000,
                UNIQUE(test_item_id, content_unit_id, environment, origin)
            );
            CREATE TABLE IF NOT EXISTS reverse_index (
                content_unit_id INTEGER NOT NULL REFERENCES content_units(id),
                test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                PRIMARY KEY (content_unit_id, test_item_id)
            );
            CREATE TABLE IF NOT EXISTS run_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                run_id TEXT NOT NULL,
                outcome TEXT NOT NULL CHECK(outcome IN ('passed','failed','skipped','flaky')),
                duration_ms INTEGER,
                error_signature TEXT,
                environment TEXT NOT NULL,
                ingested_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS environment_fingerprints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fingerprint TEXT NOT NULL UNIQUE,
                toolchain TEXT,
                os TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_reverse_lookup ON reverse_index(content_unit_id);
            CREATE INDEX IF NOT EXISTS idx_edges_test ON dependency_edges(test_item_id);
        ").map_err(|e| miette::miette!("Failed to initialize schema: {}", e))?;
        Ok(())
    }

    /// Load the selection context for a given change set.
    pub fn load_selection_context(&self, delta: &ChangeSet) -> miette::Result<SelectionContext> {
        let mut ctx = SelectionContext {
            changed: Vec::new(),
            unresolved: Vec::new(),
            cu_deps: Vec::new(),
            test_deps: Vec::new(),
            always_run: Vec::new(),
            comp_fallback: Vec::new(),
            test_comp: Vec::new(),
        };

        let mut stmt = self.conn.prepare(
            "SELECT id FROM content_units WHERE component = ?1 AND path = ?2"
        ).map_err(|e| miette::miette!("Query prep failed: {}", e))?;

        for path in &delta.files {
            let component = "default";
            let result = stmt.query_row(rusqlite::params![component, path], |row| {
                row.get::<_, u32>(0)
            });
            if let Ok(id) = result {
                ctx.changed.push(id);
            } else {
                let id = self.ensure_content_unit(component, path, None, "source")
                    .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;
                ctx.unresolved.push(id);
            }
        }

        let mut edge_stmt = self.conn.prepare(
            "SELECT de.test_item_id, de.content_unit_id, de.origin, de.k_value
             FROM dependency_edges de
             JOIN reverse_index ri ON ri.content_unit_id = de.content_unit_id
             WHERE ri.content_unit_id = ?1"
        ).map_err(|e| miette::miette!("Edge query prep failed: {}", e))?;

        for &cu_id in ctx.changed.iter().chain(ctx.unresolved.iter()) {
            let rows = edge_stmt.query_map(rusqlite::params![cu_id], |row| {
                let test_id: u32 = row.get(0)?;
                let cu_id_val: u32 = row.get(1)?;
                let origin_str: String = row.get(2)?;
                let k_val: u32 = row.get(3)?;
                let origin = match origin_str.as_str() {
                    "static" => Origin::Static,
                    "runtime" => Origin::Runtime,
                    "manual" => Origin::Manual,
                    _ => Origin::Static,
                };
                Ok((test_id, cu_id_val, origin, k_val))
            }).map_err(|e| miette::miette!("Edge query failed: {}", e))?;
            for row in rows.flatten() {
                ctx.test_deps.push(row);
            }
        }

        let mut ar_stmt = self.conn.prepare(
            "SELECT DISTINCT test_item_id FROM run_history
             WHERE outcome = 'failed' ORDER BY id DESC LIMIT 1000"
        ).map_err(|e| miette::miette!("Always-run query failed: {}", e))?;

        let rows = ar_stmt.query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Always-run exec failed: {}", e))?;
        for row in rows.flatten() {
            ctx.always_run.push(row);
        }

        Ok(ctx)
    }

    /// Ingest run results.
    pub fn ingest(&self, results: &serde_json::Value) -> miette::Result<()> {
        if let Some(tests) = results["tests"].as_array() {
            for test in tests {
                let test_id = test["id"].as_u64().unwrap_or(0) as u32;
                let outcome = test["outcome"].as_str().unwrap_or("passed");
                let duration = test["duration_ms"].as_u64();
                let run_id = results["run_id"].as_str().unwrap_or("unknown");
                self.conn.execute(
                    "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                     VALUES (?1, ?2, ?3, ?4, 'default')",
                    rusqlite::params![test_id, run_id, outcome, duration],
                ).map_err(|e| miette::miette!("Failed to insert run result: {}", e))?;
            }
        }
        Ok(())
    }

    /// Export the dependency graph as JSON.
    pub fn export_graph(&self) -> miette::Result<serde_json::Value> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut stmt = self.conn.prepare(
            "SELECT id, component, path, symbol, kind FROM content_units"
        ).map_err(|e| miette::miette!("Graph query failed: {}", e))?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, u32>(0)?,
                "component": row.get::<_, String>(1)?,
                "path": row.get::<_, String>(2)?,
                "symbol": row.get::<_, Option<String>>(3)?,
                "kind": row.get::<_, String>(4)?,
            }))
        }).map_err(|e| miette::miette!("Graph exec failed: {}", e))?;
        for row in rows.flatten() { nodes.push(row); }

        let mut estmt = self.conn.prepare(
            "SELECT test_item_id, content_unit_id, origin, k_value FROM dependency_edges"
        ).map_err(|e| miette::miette!("Edge export failed: {}", e))?;
        let erows = estmt.query_map([], |row| {
            Ok(serde_json::json!({
                "from": row.get::<_, u32>(0)?,
                "to": row.get::<_, u32>(1)?,
                "origin": row.get::<_, String>(2)?,
                "k": row.get::<_, u32>(3)?,
            }))
        }).map_err(|e| miette::miette!("Edge export exec failed: {}", e))?;
        for row in erows.flatten() { edges.push(row); }

        Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
    }

    /// Explain why a test was or was not selected.
    pub fn explain(&self, test_id: &str, _change: Option<&str>) -> miette::Result<serde_json::Value> {
        let tid: u32 = test_id.parse().unwrap_or(0);
        let mut stmt = self.conn.prepare(
            "SELECT cu.path, de.origin, de.k_value
             FROM dependency_edges de
             JOIN content_units cu ON cu.id = de.content_unit_id
             WHERE de.test_item_id = ?1"
        ).map_err(|e| miette::miette!("Explain query failed: {}", e))?;
        let deps: Vec<_> = stmt.query_map(rusqlite::params![tid], |row| {
            Ok(serde_json::json!({
                "path": row.get::<_, String>(0)?,
                "origin": row.get::<_, String>(1)?,
                "confidence": row.get::<_, u32>(2)? as f64 / 1_000_000.0,
            }))
        }).map_err(|e| miette::miette!("Explain exec failed: {}", e))?
        .flatten().collect();
        Ok(serde_json::json!({ "test_id": tid, "dependencies": deps }))
    }

    fn ensure_content_unit(&self, component: &str, path: &str, symbol: Option<&str>, kind: &str) -> rusqlite::Result<u32> {
        self.conn.execute(
            "INSERT OR IGNORE INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![component, path, symbol, kind, "unknown"],
        )?;
        self.conn.query_row(
            "SELECT id FROM content_units WHERE component = ?1 AND path = ?2 AND symbol IS ?3",
            rusqlite::params![component, path, symbol],
            |row| row.get(0),
        )
    }
}

fn find_project_root() -> miette::Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("Cannot get current directory: {}", e))?;
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join("testaruda.toml").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(cwd)
}