//! Persistence layer — SQLite store + content-addressed blob storage.
//!
//! See TIA-STORE-001 through TIA-STORE-005.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::change::ChangeSet;
use crate::engine::Origin;

/// Current schema version of the store database.
/// Increment when making breaking schema changes.
pub const SCHEMA_VERSION: u32 = 1;

/// Name of the internal schema version table.
const SCHEMA_TABLE: &str = "_schema_version";

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

        Ok(Self {
            conn,
            _db_path: db_path,
            _blob_dir: blob_dir,
        })
    }

    /// Access the underlying SQLite connection (primarily for testing).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Initialize the store schema, checking version compatibility (TIA-STORE-004).
    ///
    /// On a fresh store: creates all tables and records the schema version.
    /// On an existing store: verifies version compatibility and migrates if needed.
    /// If the store has a newer schema version than the core, refuses with a diagnostic.
    pub fn initialize(&self) -> miette::Result<()> {
        // Step 1: Create the schema version tracking table if it doesn't exist
        self.conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                version INTEGER NOT NULL
            );",
            SCHEMA_TABLE
        )).map_err(|e| miette::miette!("Failed to create schema version table: {}", e))?;

        // Step 2: Determine current schema version
        let current_version: Option<u32> = self.conn
            .query_row(
                &format!("SELECT MAX(version) FROM {}", SCHEMA_TABLE),
                [],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        match current_version {
            None => {
                // Fresh store — create all tables and record version
                self.create_schema()?;
                self.set_schema_version(SCHEMA_VERSION)?;
            }
            Some(v) if v == SCHEMA_VERSION => {
                // Schema is up to date — ensure tables exist (no-op for existing tables)
                self.create_schema()?;
            }
            Some(v) if v < SCHEMA_VERSION => {
                // Older schema — migrate forward
                eprintln!(
                    "  ℹ️  Migrating store schema from v{} to v{}...",
                    v, SCHEMA_VERSION
                );
                self.migrate(v, SCHEMA_VERSION)?;
            }
            Some(v) => {
                // Newer schema — core is too old
                return Err(miette::miette!(
                    "Store schema v{} is newer than this core (v{}). \
                     Please upgrade testaruda to use this store.",
                    v,
                    SCHEMA_VERSION
                ));
            }
        }

        Ok(())
    }

    /// Create all schema tables (idempotent — uses IF NOT EXISTS).
    fn create_schema(&self) -> miette::Result<()> {
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
            CREATE TABLE IF NOT EXISTS ingested_runs (
                run_id TEXT NOT NULL PRIMARY KEY,
                ingested_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_reverse_lookup ON reverse_index(content_unit_id);
            CREATE INDEX IF NOT EXISTS idx_edges_test ON dependency_edges(test_item_id);

            CREATE TABLE IF NOT EXISTS selection_cache (
                fingerprint TEXT NOT NULL PRIMARY KEY,
                selection_json TEXT NOT NULL,
                cached_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ").map_err(|e| miette::miette!("Failed to initialize schema: {}", e))?;
        Ok(())
    }

    /// Record the schema version in the version table.
    fn set_schema_version(&self, version: u32) -> miette::Result<()> {
        self.conn.execute(
            &format!("INSERT INTO {} (version) VALUES (?1)", SCHEMA_TABLE),
            rusqlite::params![version],
        ).map_err(|e| miette::miette!("Failed to record schema version: {}", e))?;
        Ok(())
    }

    /// Migrate the schema from an older version to the target version.
    fn migrate(&self, from: u32, to: u32) -> miette::Result<()> {
        for v in from..to {
            let next = v + 1;
            eprintln!("    Applying migration v{} → v{}...", v, next);
            self.apply_migration(v, next)?;
            self.set_schema_version(next)?;
        }
        Ok(())
    }

    /// Apply a single migration step.
    ///
    /// Each step is a version-specific transformation. For the initial schema
    /// (v0→v1), the tables already exist via CREATE IF NOT EXISTS, so this is
    /// a no-op migration that upgrades the recorded version number.
    fn apply_migration(&self, from: u32, to: u32) -> miette::Result<()> {
        match (from, to) {
            // v0 → v1: initial schema, tables already exist, nothing to transform
            (0, 1) => {
                // Ensure all tables exist (idempotent)
                self.create_schema()?;
            }
            _ => {
                return Err(miette::miette!(
                    "No migration path from v{} to v{}",
                    from,
                    to
                ));
            }
        }
        Ok(())
    }

    /// Load the selection context for a given change set.
    ///
    /// For each file in the change set, this method:
    /// 1. Computes the current content fingerprint (blake3 hash)
    /// 2. Looks up the stored fingerprint in the database
    /// 3. Classifies the content unit as:
    ///    - `unresolved`: if no prior fingerprint exists (cold-start per TIA-CHG-009)
    ///      or if the file cannot be read (conservative fallback)
    ///    - `changed`: if fingerprint differs from stored value
    ///    - Skipped: if fingerprint matches (no change — TIA-CHG-003)
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

        let mut stmt = self
            .conn
            .prepare("SELECT id, fingerprint FROM content_units WHERE component = ?1 AND path = ?2")
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;

        for path in &delta.files {
            let component = "default";
            // Resolve relative paths against the project root
            let abs_path = if Path::new(path).is_absolute() {
                Path::new(path).to_path_buf()
            } else {
                find_project_root()?.join(path)
            };
            let current_fp = Self::compute_fingerprint(&abs_path);
            let result = stmt.query_row(rusqlite::params![component, path], |row| {
                let id: u32 = row.get(0)?;
                let fingerprint: String = row.get(1)?;
                Ok((id, fingerprint))
            });
            match (result, current_fp) {
                (Ok((id, stored_fp)), Ok(fp)) if stored_fp == "unknown" || stored_fp != fp => {
                    // Cold-start (stored_fp == "unknown"): first observed content unit
                    // with no prior real fingerprint → unresolved per TIA-CHG-009
                    if stored_fp == "unknown" {
                        ctx.unresolved.push(id);
                    } else {
                        // Fingerprint changed → include in Δ per TIA-CHG-003
                        ctx.changed.push(id);
                    }
                    // Update stored fingerprint (even for cold-start: record the
                    // real fingerprint so next invocation knows it's not unknown)
                    self.conn
                        .execute(
                            "UPDATE content_units SET fingerprint = ?1 WHERE id = ?2",
                            rusqlite::params![fp, id],
                        )
                        .map_err(|e| miette::miette!("Failed to update fingerprint: {}", e))?;
                }
                (Ok((_id, _stored_fp)), Ok(_fp)) => {
                    // Fingerprint matches → unchanged, skip this file
                    // (TIA-CHG-003: matching fingerprints → excluded from Δ)
                }
                (Ok((id, _stored_fp)), Err(_)) => {
                    // File doesn't exist or can't be read → unresolved fallback
                    ctx.unresolved.push(id);
                }
                (Err(_), Ok(fp)) => {
                    // New content unit: create and store fingerprint
                    let id = self
                        .ensure_content_unit(component, path, None, "source")
                        .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;
                    self.conn
                        .execute(
                            "UPDATE content_units SET fingerprint = ?1 WHERE id = ?2",
                            rusqlite::params![fp, id],
                        )
                        .map_err(|e| miette::miette!("Failed to set fingerprint: {}", e))?;
                    ctx.unresolved.push(id); // Cold-start per TIA-CHG-009
                }
                (Err(_), Err(_)) => {
                    // Neither in store nor readable — create placeholder
                    let id = self
                        .ensure_content_unit(component, path, None, "source")
                        .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;
                    ctx.unresolved.push(id);
                }
            }
        }

        let mut edge_stmt = self
            .conn
            .prepare(
                "SELECT de.test_item_id, de.content_unit_id, de.origin, de.k_value
             FROM dependency_edges de
             JOIN reverse_index ri ON ri.content_unit_id = de.content_unit_id
             WHERE ri.content_unit_id = ?1",
            )
            .map_err(|e| miette::miette!("Edge query prep failed: {}", e))?;

        for &cu_id in ctx.changed.iter().chain(ctx.unresolved.iter()) {
            let rows = edge_stmt
                .query_map(rusqlite::params![cu_id], |row| {
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
                })
                .map_err(|e| miette::miette!("Edge query failed: {}", e))?;
            for row in rows.flatten() {
                ctx.test_deps.push(row);
            }
        }

        let mut ar_stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT test_item_id FROM run_history
             WHERE outcome = 'failed' ORDER BY id DESC LIMIT 1000",
            )
            .map_err(|e| miette::miette!("Always-run query failed: {}", e))?;

        let rows = ar_stmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Always-run exec failed: {}", e))?;
        for row in rows.flatten() {
            ctx.always_run.push(row);
        }

        Ok(ctx)
    }

    /// Ingest run results (TIA-RUN-005, TIA-REL-002).
    ///
    /// Requires a `run_id` field in the payload (run-identity key).
    /// Skips ingestion if the run_id has already been processed.
    /// Wraps all writes in a single transaction for crash safety.
    pub fn ingest(&self, results: &serde_json::Value) -> miette::Result<()> {
        // Extract run-identity key — reject if missing (TIA-RUN-005)
        let run_id = match results["run_id"].as_str() {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => {
                return Err(miette::miette!(
                    "rejected: payload missing required 'run_id' field (TIA-RUN-005)"
                ));
            }
        };

        // Check for duplicate — skip if already recorded (TIA-RUN-005)
        let existing: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM ingested_runs WHERE run_id = ?1",
                rusqlite::params![run_id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if existing {
            eprintln!("  ℹ️  Duplicate ingestion skipped for run '{}'", run_id);
            return Ok(());
        }

        // Wrap ingestion in a single transaction (TIA-REL-002)
        self.conn
            .execute_batch("BEGIN TRANSACTION")
            .map_err(|e| miette::miette!("Failed to begin transaction: {}", e))?;

        let result = (|| -> miette::Result<()> {
            // Record the run identity
            self.conn
                .execute(
                    "INSERT INTO ingested_runs (run_id) VALUES (?1)",
                    rusqlite::params![run_id],
                )
                .map_err(|e| miette::miette!("Failed to record run: {}", e))?;

            // Insert per-test results
            if let Some(tests) = results["tests"].as_array() {
                for test in tests {
                    let test_id = test["id"].as_u64().unwrap_or(0) as u32;
                    let outcome = test["outcome"].as_str().unwrap_or("passed");
                    let duration = test["duration_ms"].as_u64();
                    self.conn.execute(
                        "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                         VALUES (?1, ?2, ?3, ?4, 'default')",
                        rusqlite::params![test_id, run_id, outcome, duration],
                    )
                    .map_err(|e| miette::miette!("Failed to insert run result: {}", e))?;
                }
            }
            Ok(())
        })();

        // Commit or rollback based on success
        if result.is_ok() {
            self.conn
                .execute_batch("COMMIT")
                .map_err(|e| miette::miette!("Failed to commit transaction: {}", e))?;
        } else {
            self.conn
                .execute_batch("ROLLBACK")
                .map_err(|e| miette::miette!("Failed to rollback transaction: {}", e))?;
        }

        result
    }

    /// Store discovered test items from an adapter (TIA-ADAPT-004).
    pub fn store_test_items(
        &self,
        adapter: &str,
        items: &[crate::adapter::TestItem],
    ) -> miette::Result<()> {
        let mut stmt = self
            .conn
            .prepare(
                "INSERT OR IGNORE INTO test_items (component, adapter, node_id)
             VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| miette::miette!("Failed to prepare insert: {}", e))?;
        for item in items {
            stmt.execute(rusqlite::params!["default", adapter, item.node_id])
                .map_err(|e| miette::miette!("Failed to insert test item: {}", e))?;
        }
        Ok(())
    }

    /// Count all test items in the store.
    pub fn test_items_count(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM test_items", [], |row| row.get(0))
    }

    /// Find the project root (git repo root or first ancestor with testaruda.toml).
    pub fn find_project_root() -> miette::Result<PathBuf> {
        find_project_root()
    }

    /// Store static dependency edges from an adapter (TIA-ADAPT-005).
    ///
    /// Creates content units for edges and inserts dependency edges into the
    /// dependency_edges and reverse_index tables.
    pub fn store_static_deps(
        &self,
        adapter: &str,
        deps: &[crate::adapter::DepEdge],
    ) -> miette::Result<()> {
        for edge in deps {
            // Ensure the content unit (target) exists
            let cu_id = self
                .ensure_content_unit("default", &edge.to, None, "source")
                .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;

            // Find the test item (source) by node_id
            let test_id = self.conn.query_row(
                "SELECT id FROM test_items WHERE component = 'default' AND adapter = ?1 AND node_id = ?2",
                rusqlite::params![adapter, edge.from],
                |row| row.get::<_, u32>(0),
            );
            if let Ok(tid) = test_id {
                // Insert the dependency edge
                let origin = match edge.origin.as_str() {
                    "runtime" => "runtime",
                    "manual" => "manual",
                    _ => "static",
                };
                self.conn.execute(
                    "INSERT OR IGNORE INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
                     VALUES (?1, ?2, '', ?3, ?4)",
                    rusqlite::params![tid, cu_id, origin, edge.weight],
                ).map_err(|e| miette::miette!("Failed to insert dependency edge: {}", e))?;

                // Also update the reverse index
                self.conn
                    .execute(
                        "INSERT OR IGNORE INTO reverse_index (content_unit_id, test_item_id)
                     VALUES (?1, ?2)",
                        rusqlite::params![cu_id, tid],
                    )
                    .map_err(|e| miette::miette!("Failed to update reverse index: {}", e))?;
            }
        }
        Ok(())
    }

    /// Update the fingerprint for a content unit identified by path.
    /// Used by adapter fingerprint command integration (TIA-ADAPT-006).
    pub fn update_fingerprint(&self, path: &str, fingerprint: &str) -> miette::Result<()> {
        self.conn
            .execute(
                "UPDATE content_units SET fingerprint = ?1 WHERE path = ?2",
                rusqlite::params![fingerprint, path],
            )
            .map_err(|e| miette::miette!("Failed to update fingerprint: {}", e))?;
        Ok(())
    }

    /// Export the dependency graph as JSON.
    pub fn export_graph(&self) -> miette::Result<serde_json::Value> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let mut stmt = self
            .conn
            .prepare("SELECT id, component, path, symbol, kind FROM content_units")
            .map_err(|e| miette::miette!("Graph query failed: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, u32>(0)?,
                    "component": row.get::<_, String>(1)?,
                    "path": row.get::<_, String>(2)?,
                    "symbol": row.get::<_, Option<String>>(3)?,
                    "kind": row.get::<_, String>(4)?,
                }))
            })
            .map_err(|e| miette::miette!("Graph exec failed: {}", e))?;
        for row in rows.flatten() {
            nodes.push(row);
        }

        let mut estmt = self
            .conn
            .prepare("SELECT test_item_id, content_unit_id, origin, k_value FROM dependency_edges")
            .map_err(|e| miette::miette!("Edge export failed: {}", e))?;
        let erows = estmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "from": row.get::<_, u32>(0)?,
                    "to": row.get::<_, u32>(1)?,
                    "origin": row.get::<_, String>(2)?,
                    "k": row.get::<_, u32>(3)?,
                }))
            })
            .map_err(|e| miette::miette!("Edge export exec failed: {}", e))?;
        for row in erows.flatten() {
            edges.push(row);
        }

        Ok(serde_json::json!({ "nodes": nodes, "edges": edges }))
    }

    /// Explain why a test was or was not selected.
    pub fn explain(
        &self,
        test_id: &str,
        _change: Option<&str>,
    ) -> miette::Result<serde_json::Value> {
        let tid: u32 = test_id.parse().unwrap_or(0);
        let mut stmt = self
            .conn
            .prepare(
                "SELECT cu.path, de.origin, de.k_value
             FROM dependency_edges de
             JOIN content_units cu ON cu.id = de.content_unit_id
             WHERE de.test_item_id = ?1",
            )
            .map_err(|e| miette::miette!("Explain query failed: {}", e))?;
        let deps: Vec<_> = stmt
            .query_map(rusqlite::params![tid], |row| {
                Ok(serde_json::json!({
                    "path": row.get::<_, String>(0)?,
                    "origin": row.get::<_, String>(1)?,
                    "confidence": row.get::<_, u32>(2)? as f64 / 1_000_000.0,
                }))
            })
            .map_err(|e| miette::miette!("Explain exec failed: {}", e))?
            .flatten()
            .collect();
        Ok(serde_json::json!({ "test_id": tid, "dependencies": deps }))
    }

    /// Compute a blake3 content fingerprint for the file at `path`.
    ///
    /// Returns `Err` with the IO error if the file cannot be read (missing,
    /// permissions, etc.) as a conservative fallback — the caller classifies
    /// as unresolved.
    pub fn compute_fingerprint(path: &Path) -> Result<String, std::io::Error> {
        let data = std::fs::read(path)?;
        let hash = blake3::hash(&data);
        Ok(hash.to_hex().to_string())
    }

    fn ensure_content_unit(
        &self,
        component: &str,
        path: &str,
        symbol: Option<&str>,
        kind: &str,
    ) -> rusqlite::Result<u32> {
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

    /// Compute a fingerprint for a component by hashing all its content unit fingerprints.
    ///
    /// Used as a cache key for cached selection decisions (TIA-COMP-010).
    pub fn compute_component_fingerprint(&self, component: &str) -> miette::Result<String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT fingerprint FROM content_units WHERE component = ?1 ORDER BY path, symbol",
            )
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;
        let fps: Vec<String> = stmt
            .query_map(rusqlite::params![component], |row| row.get(0))
            .map_err(|e| miette::miette!("Query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        let combined = fps.join("|");
        let hash = blake3::hash(combined.as_bytes());
        Ok(hash.to_hex().to_string())
    }

    /// Look up a cached selection by component fingerprint (TIA-COMP-010).
    pub fn get_cached_selection(&self, fingerprint: &str) -> miette::Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT selection_json FROM selection_cache WHERE fingerprint = ?1",
            rusqlite::params![fingerprint],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json) => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(miette::miette!("Cache lookup failed: {}", e)),
        }
    }

    /// Store a selection result in the cache (TIA-COMP-010).
    pub fn set_cached_selection(
        &self,
        fingerprint: &str,
        selection_json: &str,
    ) -> miette::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO selection_cache (fingerprint, selection_json, cached_at)
             VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![fingerprint, selection_json],
        ).map_err(|e| miette::miette!("Failed to cache selection: {}", e))?;
        Ok(())
    }

    /// Invalidate all cached selections for a component (TIA-COMP-010).
    pub fn invalidate_component_cache(&self, _component: &str) -> miette::Result<()> {
        // Since the cache key is a global fingerprint, we clear all entries
        // when any component's cache is invalidated. In a future iteration
        // with per-component cache keys, this would be more targeted.
        self.conn
            .execute("DELETE FROM selection_cache", [])
            .map_err(|e| miette::miette!("Failed to invalidate cache: {}", e))?;
        Ok(())
    }

    /// Load the mean recorded duration (in ms) for each test item that has
    /// run history. Returns a map of test_item_id → mean_duration_ms.
    ///
    /// Used by duration-based ordering (TIA-SEL-006).
    pub fn load_mean_durations(&self) -> miette::Result<std::collections::HashMap<u32, u64>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT test_item_id, CAST(ROUND(AVG(duration_ms)) AS INTEGER)
                 FROM run_history
                 WHERE duration_ms IS NOT NULL
                 GROUP BY test_item_id",
            )
            .map_err(|e| miette::miette!("Duration query prep failed: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                let id: u32 = row.get(0)?;
                let avg: u64 = row.get(1)?;
                Ok((id, avg))
            })
            .map_err(|e| miette::miette!("Duration query failed: {}", e))?;
        let mut map = std::collections::HashMap::new();
        for row in rows.flatten() {
            map.insert(row.0, row.1);
        }
        Ok(map)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_compute_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("content.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello world").unwrap();
        drop(f);

        let fp = Store::compute_fingerprint(&path).unwrap();
        // blake3("hello world")
        assert_eq!(fp.len(), 64, "blake3 hex should be 64 chars");
        assert!(
            fp.chars().all(|c| c.is_ascii_hexdigit()),
            "blake3 hex should be hex digits"
        );

        // Same content should produce same hash
        let fp2 = Store::compute_fingerprint(&path).unwrap();
        assert_eq!(fp, fp2);

        // Changed content should produce different hash
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"goodbye world").unwrap();
        drop(f);
        let fp3 = Store::compute_fingerprint(&path).unwrap();
        assert_ne!(fp, fp3);
    }

    #[test]
    fn test_compute_fingerprint_missing_file() {
        let result = Store::compute_fingerprint(Path::new("/nonexistent/file.rs"));
        assert!(result.is_err());
    }

    #[test]
    fn test_fingerprint_update_on_selection() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        let store = Store::open(store_path.clone()).unwrap();
        store.initialize().unwrap();

        // Create a test file inside the temp dir
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();

        // Use an absolute path so compute_fingerprint can find it
        let abs_path = file_path.to_string_lossy().to_string();

        // First selection: cold-start — file is new, should be unresolved
        let delta = ChangeSet {
            files: vec![abs_path.clone()],
            base: None,
            head: None,
        };
        let ctx = store.load_selection_context(&delta).unwrap();
        assert_eq!(ctx.changed.len(), 0, "cold-start should not be changed");
        assert_eq!(ctx.unresolved.len(), 1, "cold-start should be unresolved");

        // Verify the fingerprint was updated from 'unknown' to a real hash
        let fp: String = store
            .conn
            .query_row(
                "SELECT fingerprint FROM content_units WHERE path = ?1",
                rusqlite::params![abs_path],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(fp, "unknown", "fingerprint should be updated from unknown");
        assert_eq!(fp.len(), 64);

        // Second selection with same content: should be unchanged (empty Δ)
        let ctx2 = store.load_selection_context(&delta).unwrap();
        assert_eq!(ctx2.changed.len(), 0, "unchanged file should not be in Δ");
        assert_eq!(
            ctx2.unresolved.len(),
            0,
            "unchanged file should not be unresolved"
        );

        // Modify the file
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        // Third selection: changed content → should be in Δ
        let ctx3 = store.load_selection_context(&delta).unwrap();
        assert_eq!(ctx3.changed.len(), 1, "modified file should be changed");
        assert_eq!(ctx3.unresolved.len(), 0);
    }

    #[test]
    fn test_missing_file_returns_unresolved() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Delta includes an absolute path that doesn't exist on disk
        let missing_path = dir.path().join("gone.rs").to_string_lossy().to_string();
        let delta = ChangeSet {
            files: vec![missing_path],
            base: None,
            head: None,
        };
        let ctx = store.load_selection_context(&delta).unwrap();
        assert_eq!(ctx.changed.len(), 0);
        assert_eq!(ctx.unresolved.len(), 1, "missing file should be unresolved");
    }

    #[test]
    fn test_fresh_store_has_current_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let version: u32 = store
            .conn
            .query_row(
                &format!("SELECT MAX(version) FROM {}", SCHEMA_TABLE),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "fresh store should have current version");
    }

    #[test]
    fn test_schema_version_persists_across_opens() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");

        // First open: initialize
        {
            let store = Store::open(store_path.clone()).unwrap();
            store.initialize().unwrap();
        }

        // Second open: verify version is still correct
        {
            let store = Store::open(store_path.clone()).unwrap();
            store.initialize().unwrap(); // should succeed without migration

            let version: u32 = store
                .conn
                .query_row(
                    &format!("SELECT MAX(version) FROM {}", SCHEMA_TABLE),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, SCHEMA_VERSION);
        }
    }

    #[test]
    fn test_older_schema_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");

        // Create a store with version 0 (older schema)
        {
            let store = Store::open(store_path.clone()).unwrap();
            store.initialize().unwrap();

            // Overwrite the schema version to simulate an older store
            store
                .conn
                .execute(
                    &format!("INSERT INTO {} (version) VALUES (0)", SCHEMA_TABLE),
                    [],
                )
                .unwrap();
        }

        // Reopen and re-initialize — should migrate forward
        {
            let store = Store::open(store_path.clone()).unwrap();
            store.initialize().unwrap();

            let version: u32 = store
                .conn
                .query_row(
                    &format!("SELECT MAX(version) FROM {}", SCHEMA_TABLE),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, SCHEMA_VERSION, "older schema should be migrated to current");
        }
    }

    #[test]
    fn test_newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");

        // Create a store with schema version higher than current
        {
            let store = Store::open(store_path.clone()).unwrap();
            store.initialize().unwrap();

            // Overwrite to simulate a future version
            store
                .conn
                .execute(
                    &format!("DELETE FROM {}", SCHEMA_TABLE),
                    [],
                )
                .unwrap();
            store
                .conn
                .execute(
                    &format!("INSERT INTO {} (version) VALUES (?1)", SCHEMA_TABLE),
                    rusqlite::params![SCHEMA_VERSION + 99],
                )
                .unwrap();
        }

        // Reopen and re-initialize — should refuse
        {
            let store = Store::open(store_path.clone()).unwrap();
            let result = store.initialize();
            assert!(result.is_err(), "newer schema should be refused");
            let err = result.unwrap_err().to_string();
            assert!(err.contains("newer"), "error should mention 'newer'");
            assert!(err.contains("upgrade"), "error should mention 'upgrade'");
        }
    }

    #[test]
    fn test_schema_constant_is_positive() {
        assert!(SCHEMA_VERSION > 0, "schema version must be positive");
    }

    #[test]
    fn test_migrate_unknown_path_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Directly call apply_migration with an unknown path
        let result = store.apply_migration(1, 2);
        assert!(
            result.is_err(),
            "unknown migration path should return error"
        );
    }

    #[test]
    fn test_cache_store_and_retrieve() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let fp = "deadbeef12345678";
        let json = r#"{"tests":[{"id":1,"confidence":1.0}]}"#;

        // Initially empty
        let cached = store.get_cached_selection(fp).unwrap();
        assert!(cached.is_none(), "cache should be empty initially");

        // Store
        store.set_cached_selection(fp, json).unwrap();

        // Retrieve
        let cached = store.get_cached_selection(fp).unwrap();
        assert_eq!(cached.as_deref(), Some(json));
    }

    #[test]
    fn test_cache_overwrite_and_miss() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        store
            .set_cached_selection("fp1", r#"{"sel":1}"#)
            .unwrap();
        store
            .set_cached_selection("fp2", r#"{"sel":2}"#)
            .unwrap();

        // Each fingerprint retrieves its own
        assert_eq!(
            store.get_cached_selection("fp1").unwrap().as_deref(),
            Some(r#"{"sel":1}"#)
        );
        assert_eq!(
            store.get_cached_selection("fp2").unwrap().as_deref(),
            Some(r#"{"sel":2}"#)
        );

        // Overwrite fp1
        store
            .set_cached_selection("fp1", r#"{"sel":99}"#)
            .unwrap();
        assert_eq!(
            store.get_cached_selection("fp1").unwrap().as_deref(),
            Some(r#"{"sel":99}"#)
        );

        // Unknown fingerprint returns None
        assert!(store.get_cached_selection("unknown").unwrap().is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        store
            .set_cached_selection("fp_a", r#"{}"#)
            .unwrap();
        store
            .set_cached_selection("fp_b", r#"{}"#)
            .unwrap();

        // Invalidate (clears all, since cache key is global)
        store.invalidate_component_cache("default").unwrap();

        assert!(store.get_cached_selection("fp_a").unwrap().is_none());
        assert!(store.get_cached_selection("fp_b").unwrap().is_none());
    }

    #[test]
    fn test_component_fingerprint_empty_component() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Empty component should produce a deterministic hash
        let fp = store.compute_component_fingerprint("empty").unwrap();
        assert_eq!(fp.len(), 64, "blake3 hex should be 64 chars");
        // Same empty component should produce same hash
        let fp2 = store.compute_component_fingerprint("empty").unwrap();
        assert_eq!(fp, fp2);
    }

    #[test]
    fn test_ingest_missing_run_id_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Payload without run_id should be rejected
        let payload = serde_json::json!({"tests": []});
        let result = store.ingest(&payload);
        assert!(result.is_err(), "missing run_id should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("run_id"),
            "error should mention run_id: {}",
            err
        );
    }

    #[test]
    fn test_ingest_duplicate_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Insert a test item so the FK constraint is satisfied
        store.conn().execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_node')",
            [],
        ).unwrap();

        let payload = serde_json::json!({
            "run_id": "test-run-001",
            "tests": [{"id": 1, "outcome": "passed", "duration_ms": 10}]
        });

        // First ingest should succeed
        assert!(store.ingest(&payload).is_ok(), "first ingest should succeed");

        // Second ingest with same run_id should skip (not error)
        assert!(store.ingest(&payload).is_ok(), "duplicate should not error");

        // Verify only one run_history entry exists
        let count: u32 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM run_history WHERE run_id = ?1",
                rusqlite::params!["test-run-001"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "only one run_history entry for duplicate");
    }

    #[test]
    fn test_ingest_transaction_rollback_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Insert a test item for FK constraint
        store.conn().execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_node2')",
            [],
        ).unwrap();

        // Payload with an invalid outcome (CHECK constraint violation)
        let payload = serde_json::json!({
            "run_id": "crash-test",
            "tests": [{"id": 1, "outcome": "invalid_outcome", "duration_ms": 10}]
        });

        let result = store.ingest(&payload);
        assert!(result.is_err(), "invalid outcome should cause error");

        // Verify the run was NOT recorded (transaction rolled back)
        let exists: bool = store
            .conn()
            .query_row(
                "SELECT 1 FROM ingested_runs WHERE run_id = ?1",
                rusqlite::params!["crash-test"],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(!exists, "ingested_runs should be empty after rollback");
    }

    #[test]
    fn test_ingest_empty_run_id_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Empty string run_id should also be rejected
        let payload = serde_json::json!({
            "run_id": "",
            "tests": []
        });
        let result = store.ingest(&payload);
        assert!(result.is_err(), "empty run_id should be rejected");
    }
}
