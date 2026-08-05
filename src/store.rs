//! Persistence layer — SQLite store + content-addressed blob storage.
//!
//! See TIA-STORE-001 through TIA-STORE-005.

use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::change::ChangeSet;
use crate::config::Config;
use crate::engine::Origin;
use crate::ONE;

/// Current schema version of the store database.
/// Increment when making breaking schema changes.
pub const SCHEMA_VERSION: u32 = 4;

/// Name of the internal schema version table.
const SCHEMA_TABLE: &str = "_schema_version";

/// The store holds the dependency graph, test history, and run payloads.
pub struct Store {
    conn: Connection,
    _db_path: PathBuf,
    _blob_dir: PathBuf,
    project_root: PathBuf,
}

/// Metrics from evaluating the predictive ranking model against held-out data.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct CalibrationMetrics {
    /// Number of distinct test items in the hold-out set.
    pub total_test_items: usize,
    /// Number of tests that failed in the hold-out set.
    pub total_failures: usize,
    /// Number of failures captured in the top-k of the ranked prediction.
    pub captured_failures: usize,
    /// recall@k = captured_failures / total_failures (0.0 if no failures).
    pub recall_at_k: f64,
    /// k value: number of tests in the hold-out set.
    pub k: usize,
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
    pub quarantined: Vec<u32>,
    /// Current environment fingerprint for scoped queries (TIA-CORE-008).
    pub current_environment: String,
    /// Invocation-level quality multiplier in ppm (TIA-CONF-002).
    /// Derived from coverage freshness, adapter resolution, history depth,
    /// and environment match. Applied to all path-based confidence computations.
    pub invocation_quality: u32,
    /// Confidence threshold in ppm (TIA-CONF-002, TIA-SAFE-002).
    /// Confidence threshold in ppm (TIA-CONF-002, TIA-SAFE-002).
    /// If the minimum Viterbi path confidence across reachability-selected
    /// tests in a component falls below this threshold, all tests in that
    /// component are selected (component-scoped fallback).
    pub confidence_threshold: u32,
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

        // Project root is the parent of the store directory
        let project_root = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| path.clone());

        Ok(Self {
            conn,
            _db_path: db_path,
            _blob_dir: blob_dir,
            project_root,
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
        self.conn
            .execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {} (
                version INTEGER NOT NULL
            );",
                SCHEMA_TABLE
            ))
            .map_err(|e| miette::miette!("Failed to create schema version table: {}", e))?;

        // Step 2: Determine current schema version
        let current_version: Option<u32> = self
            .conn
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
                quarantined INTEGER NOT NULL DEFAULT 0,
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

            CREATE TABLE IF NOT EXISTS provenance (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                selected INTEGER NOT NULL CHECK(selected IN (0, 1)),
                confidence REAL NOT NULL DEFAULT 0.0,
                distance INTEGER,
                witness_json TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(run_id, test_item_id)
            );

            CREATE INDEX IF NOT EXISTS idx_provenance_run ON provenance(run_id);
            CREATE INDEX IF NOT EXISTS idx_provenance_test ON provenance(test_item_id);

            CREATE TABLE IF NOT EXISTS missed_selection_incidents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                implicated_content_unit_id INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_missed_sel_run ON missed_selection_incidents(run_id);

            -- Content unit uniqueness: partial indexes handle NULL symbol (testaruda-p37i)
            CREATE UNIQUE INDEX IF NOT EXISTS ux_cu_path
                ON content_units(component, path) WHERE symbol IS NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS ux_cu_path_sym
                ON content_units(component, path, symbol) WHERE symbol IS NOT NULL;").map_err(|e| miette::miette!("Failed to initialize schema: {}", e))?;
        Ok(())
    }

    /// Record the schema version in the version table.
    fn set_schema_version(&self, version: u32) -> miette::Result<()> {
        self.conn
            .execute(
                &format!("INSERT INTO {} (version) VALUES (?1)", SCHEMA_TABLE),
                rusqlite::params![version],
            )
            .map_err(|e| miette::miette!("Failed to record schema version: {}", e))?;
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
            // v1 → v2: add quarantined column to test_items (TIA-SAFE-010)
            (1, 2) => {
                self.conn
                    .execute_batch(
                        "ALTER TABLE test_items ADD COLUMN quarantined INTEGER NOT NULL DEFAULT 0;",
                    )
                    .map_err(|e| miette::miette!("Failed to add quarantined column: {}", e))?;
            }
            // v2 → v3: add missed_selection_incidents table (TIA-SAFE-008)
            (2, 3) => {
                self.conn
                    .execute_batch(
                        "CREATE TABLE IF NOT EXISTS missed_selection_incidents (
                            id INTEGER PRIMARY KEY AUTOINCREMENT,
                            run_id TEXT NOT NULL,
                            test_item_id INTEGER NOT NULL REFERENCES test_items(id),
                            implicated_content_unit_id INTEGER NOT NULL,
                            created_at TEXT NOT NULL DEFAULT (datetime('now'))
                        );
                        CREATE INDEX IF NOT EXISTS idx_missed_sel_run ON missed_selection_incidents(run_id);",
                    )
                    .map_err(|e| miette::miette!("Failed to create missed_selection_incidents table: {}", e))?;
            }
            // v3 → v4: add partial unique indexes for content_units (testaruda-p37i)
            (3, 4) => {
                self.conn
                    .execute_batch(
                        "CREATE TABLE IF NOT EXISTS content_units (
                            id INTEGER PRIMARY KEY AUTOINCREMENT,
                            component TEXT NOT NULL,
                            path TEXT NOT NULL,
                            symbol TEXT,
                            kind TEXT NOT NULL CHECK(kind IN ('source','config','fixture','lockfile','external')),
                            fingerprint TEXT NOT NULL,
                            UNIQUE(component, path, symbol)
                        );
                        -- Remove duplicates: keep first row for each (component, path, NULL) group
                        DELETE FROM content_units WHERE id NOT IN (
                            SELECT MIN(id) FROM content_units GROUP BY component, path, COALESCE(symbol, '')
                        );
                        -- Add partial unique indexes for NULL-safe uniqueness
                        CREATE UNIQUE INDEX IF NOT EXISTS ux_cu_path
                            ON content_units(component, path) WHERE symbol IS NULL;
                        CREATE UNIQUE INDEX IF NOT EXISTS ux_cu_path_sym
                            ON content_units(component, path, symbol) WHERE symbol IS NOT NULL;",
                    )
                    .map_err(|e| miette::miette!("Failed to add content_unit unique indexes: {}", e))?;
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

    /// Generate a unique run ID for a selection invocation.
    pub fn generate_run_id(&self) -> miette::Result<String> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Ok(format!("sel-{:016x}", nanos))
    }

    /// Persist provenance for a selection run.
    ///
    /// Stores one row per test in the selection. For selected tests, includes
    /// confidence, distance, and witness chain. Non-selected tests in the
    /// candidate set are stored with selected=0 and an inferred exclusion reason.
    pub fn persist_provenance(
        &self,
        run_id: &str,
        selection: &crate::engine::Selection,
        candidate_test_ids: &[u32],
    ) -> miette::Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| miette::miette!("Failed to start provenance transaction: {}", e))?;

        let mut stmt = tx
            .prepare(
                "INSERT OR REPLACE INTO provenance (run_id, test_item_id, selected, confidence, distance, witness_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| miette::miette!("Provenance insert prep failed: {}", e))?;

        // Build a set of selected test IDs for fast lookup
        use std::collections::HashSet;
        let selected_ids: HashSet<u32> = selection.tests.iter().map(|t| t.id).collect();

        // Persist selected tests
        for test in &selection.tests {
            let witness_json = test
                .witness
                .as_ref()
                .map(|w| serde_json::to_string(w).unwrap_or_default())
                .unwrap_or_default();
            stmt.execute(rusqlite::params![
                run_id,
                test.id,
                1i32,
                test.confidence,
                test.distance,
                witness_json,
            ])
            .map_err(|e| {
                miette::miette!("Failed to persist provenance for test {}: {}", test.id, e)
            })?;
        }

        // Persist non-selected candidates with exclusion marker
        for &tid in candidate_test_ids {
            if !selected_ids.contains(&tid) {
                stmt.execute(rusqlite::params![
                    run_id,
                    tid,
                    0i32,
                    0.0f64,
                    None::<u32>,
                    r#"{"reason":"no change reaches test"}"#,
                ])
                .map_err(|e| {
                    miette::miette!("Failed to persist exclusion for test {}: {}", tid, e)
                })?;
            }
        }

        drop(stmt);
        tx.commit()
            .map_err(|e| miette::miette!("Failed to commit provenance: {}", e))?;
        Ok(())
    }

    /// Retrieve provenance for a specific run and test.
    pub fn get_provenance_entry(
        &self,
        run_id: &str,
        test_id: u32,
    ) -> miette::Result<Option<serde_json::Value>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT selected, confidence, distance, witness_json, created_at
                 FROM provenance WHERE run_id = ?1 AND test_item_id = ?2",
            )
            .map_err(|e| miette::miette!("Provenance query prep failed: {}", e))?;

        let result = stmt.query_row(rusqlite::params![run_id, test_id], |row| {
            let selected: i32 = row.get(0)?;
            let confidence: f64 = row.get(1)?;
            let distance: Option<u32> = row.get(2)?;
            let witness_json: Option<String> = row.get(3)?;
            let created_at: String = row.get(4)?;
            let witness =
                witness_json.and_then(|w| serde_json::from_str::<serde_json::Value>(&w).ok());
            Ok(serde_json::json!({
                "selected": selected != 0,
                "confidence": confidence,
                "distance": distance,
                "witness": witness,
                "created_at": created_at,
            }))
        });

        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(miette::miette!("Provenance query failed: {}", e)),
        }
    }

    /// Detect and record missed-selection incidents (TIA-SAFE-008).
    ///
    /// After ingesting a full run, compares the results against the last
    /// selection's provenance. For every test that failed in the full run
    /// but was skipped by the last selection, records a missed-selection
    /// incident and creates a `manual` edge so the test is forced on the
    /// implicated change in future selections.
    ///
    /// Returns the number of incidents recorded.
    pub fn detect_missed_selections(
        &self,
        run_id: &str,
        results: &serde_json::Value,
    ) -> miette::Result<usize> {
        // Get the last selection run ID from provenance
        let last_sel_run: Option<String> = self
            .conn
            .query_row(
                "SELECT run_id FROM provenance ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let Some(prev_run_id) = last_sel_run else {
            // No previous selection to compare against
            return Ok(0);
        };

        // Build a set of what the last selection would have skipped
        let mut skip_stmt = self
            .conn
            .prepare("SELECT test_item_id FROM provenance WHERE run_id = ?1 AND selected = 0")
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;
        let skipped_ids: std::collections::HashSet<u32> = skip_stmt
            .query_map(rusqlite::params![prev_run_id], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Query exec failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        if skipped_ids.is_empty() {
            return Ok(0);
        }

        // Get the last selection's changed content units (for implicated cu)
        let mut changed_stmt = self
            .conn
            .prepare(
                "SELECT test_item_id, witness_json FROM provenance WHERE run_id = ?1 AND selected = 1",
            )
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;
        let witness_map: std::collections::HashMap<u32, Vec<u32>> = changed_stmt
            .query_map(rusqlite::params![prev_run_id], |row| {
                let tid: u32 = row.get(0)?;
                let wjson: Option<String> = row.get(1)?;
                Ok((tid, wjson))
            })
            .map_err(|e| miette::miette!("Query exec failed: {}", e))?
            .filter_map(|r| r.ok())
            .map(|(tid, wjson)| {
                let cus = wjson
                    .and_then(|w| serde_json::from_str::<Vec<crate::engine::WitnessEdge>>(&w).ok())
                    .unwrap_or_default()
                    .iter()
                    .map(|e| e.content_unit)
                    .collect();
                (tid, cus)
            })
            .collect();

        // Collect all content units implicated in the last selection
        let all_implicated_cus: Vec<u32> = witness_map
            .values()
            .flat_map(|v| v.iter())
            .copied()
            .collect();

        let mut incidents = 0;

        // Check each test in the ingested run
        if let Some(tests) = results["tests"].as_array() {
            for test in tests {
                let test_id = test["id"].as_u64().unwrap_or(0) as u32;
                let outcome = test["outcome"].as_str().unwrap_or("passed");

                // Only care about tests that failed and were skipped by last selection
                if outcome == "failed" && skipped_ids.contains(&test_id) {
                    // Determine the implicated content unit: use the first one
                    // from witness data, or the first from all_implicated_cus
                    let implicated_cu = witness_map
                        .get(&test_id)
                        .and_then(|cus| cus.first().copied())
                        .or_else(|| all_implicated_cus.first().copied())
                        .unwrap_or(0);

                    self.record_missed_selection(run_id, test_id, implicated_cu)?;
                    incidents += 1;
                }
            }
        }

        Ok(incidents)
    }

    /// Record a single missed-selection incident and create a manual edge.
    /// Detect flaky tests by examining outcome oscillation across recent runs
    /// (TIA-SAFE-011). A test is flaky if it has both `passed` and `failed`
    /// outcomes in its last N runs. Marked as quarantined (TIA-SAFE-010) to
    /// ensure they are always selected and excluded from confidence scoring.
    fn detect_flaky_tests(&self) -> miette::Result<()> {
        // Find tests with both passed and failed outcomes in their last 5 runs
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.id FROM test_items t
             WHERE (
                 SELECT COUNT(DISTINCT outcome) FROM (
                     SELECT outcome FROM run_history
                     WHERE test_item_id = t.id
                     ORDER BY id DESC
                     LIMIT 5
                 )
             ) > 1
             AND t.quarantined = 0",
            )
            .map_err(|e| miette::miette!("Failed to prepare flaky query: {}", e))?;

        let flaky_tests: Vec<u32> = stmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Failed to query flaky tests: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        for test_id in &flaky_tests {
            self.conn
                .execute(
                    "UPDATE test_items SET quarantined = 1 WHERE id = ?1",
                    rusqlite::params![test_id],
                )
                .map_err(|e| {
                    miette::miette!("Failed to quarantine flaky test {}: {}", test_id, e)
                })?;

            // Also record the most recent outcome as flaky in run_history
            // for the last run that had this test
            if let Ok(node_id) = self.get_test_node_id(*test_id) {
                eprintln!(
                    "  🟡 Flaky test detected: {} (id={}) — quarantined",
                    node_id, test_id
                );
            }
        }

        Ok(())
    }

    fn record_missed_selection(
        &self,
        run_id: &str,
        test_id: u32,
        implicated_cu: u32,
    ) -> miette::Result<()> {
        // Insert incident record
        self.conn
            .execute(
                "INSERT INTO missed_selection_incidents (run_id, test_item_id, implicated_content_unit_id)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![run_id, test_id, implicated_cu],
            )
            .map_err(|e| miette::miette!("Failed to record incident: {}", e))?;

        // Create a manual edge that forces the test on the implicated change
        // (k_value = ONE = max confidence)
        self.conn
            .execute(
                "INSERT OR REPLACE INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
                 VALUES (?1, ?2, 'default', 'manual', ?3)",
                rusqlite::params![test_id, implicated_cu, crate::ONE],
            )
            .map_err(|e| miette::miette!("Failed to create manual edge: {}", e))?;

        // Also add to reverse_index if not already there
        self.conn
            .execute(
                "INSERT OR IGNORE INTO reverse_index (content_unit_id, test_item_id)
                 VALUES (?1, ?2)",
                rusqlite::params![implicated_cu, test_id],
            )
            .map_err(|e| miette::miette!("Failed to update reverse index: {}", e))?;

        eprintln!(
            "  ⚠️  Missed-selection incident: test {} failed but was skipped by last selection",
            test_id
        );

        Ok(())
    }

    /// Get the most recent run ID from provenance.
    pub fn get_latest_run_id(&self) -> miette::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT run_id FROM provenance ORDER BY id DESC LIMIT 1")
            .map_err(|e| miette::miette!("Latest run query prep failed: {}", e))?;

        match stmt.query_row([], |row| row.get::<_, String>(0)) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(miette::miette!("Latest run query failed: {}", e)),
        }
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
            quarantined: Vec::new(),
            current_environment: "default".to_string(),
            invocation_quality: ONE,
            confidence_threshold: ONE,
        };

        // Load config early for environment (needed by edge query below)
        let config = Config::load_or_default(&self.project_root);
        if !config.environment.name.is_empty() {
            ctx.current_environment = config.environment.name.clone();
        }

        self.process_file_fingerprints(delta, &mut ctx)?;
        self.load_dependency_edges(&mut ctx)?;
        self.load_failed_tests(&mut ctx)?;
        self.load_no_history_tests(&mut ctx)?;
        self.load_quarantined_tests(&mut ctx)?;
        ctx.invocation_quality = self.compute_invocation_quality();

        let threshold = config.confidence_threshold;
        ctx.confidence_threshold = (threshold * ONE as f64) as u32;

        self.apply_must_run_rules(&config, delta, &mut ctx);
        self.check_periodic_full_run(&config, &mut ctx)?;

        Ok(ctx)
    }

    /// Process file fingerprints: compare stored vs current fingerprints
    /// and populate ctx.changed / ctx.unresolved accordingly.
    fn process_file_fingerprints(
        &self,
        delta: &ChangeSet,
        ctx: &mut SelectionContext,
    ) -> miette::Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, fingerprint FROM content_units WHERE component = ?1 AND path = ?2")
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;

        for path in &delta.files {
            let component = "default";
            let abs_path = if Path::new(path).is_absolute() {
                Path::new(path).to_path_buf()
            } else {
                self.project_root.join(path)
            };
            let current_fp = Self::compute_fingerprint(&abs_path);
            let result = stmt.query_row(rusqlite::params![component, path], |row| {
                let id: u32 = row.get(0)?;
                let fingerprint: String = row.get(1)?;
                Ok((id, fingerprint))
            });
            match (result, current_fp) {
                (Ok((id, stored_fp)), Ok(fp)) if stored_fp == "unknown" || stored_fp != fp => {
                    if stored_fp == "unknown" {
                        ctx.unresolved.push(id);
                    } else {
                        ctx.changed.push(id);
                    }
                    self.conn
                        .execute(
                            "UPDATE content_units SET fingerprint = ?1 WHERE id = ?2",
                            rusqlite::params![fp, id],
                        )
                        .map_err(|e| miette::miette!("Failed to update fingerprint: {}", e))?;
                }
                (Ok((_id, _stored_fp)), Ok(_fp)) => {}
                (Ok((id, _stored_fp)), Err(_)) => {
                    ctx.unresolved.push(id);
                }
                (Err(_), Ok(fp)) => {
                    let id = self
                        .ensure_content_unit(component, path, None, "source")
                        .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;
                    self.conn
                        .execute(
                            "UPDATE content_units SET fingerprint = ?1 WHERE id = ?2",
                            rusqlite::params![fp, id],
                        )
                        .map_err(|e| miette::miette!("Failed to set fingerprint: {}", e))?;
                    ctx.unresolved.push(id);
                }
                (Err(_), Err(_)) => {
                    let id = self
                        .ensure_content_unit(component, path, None, "source")
                        .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;
                    ctx.unresolved.push(id);
                }
            }
        }
        Ok(())
    }

    /// Load dependency edges for all changed and unresolved content units.
    fn load_dependency_edges(&self, ctx: &mut SelectionContext) -> miette::Result<()> {
        let mut edge_stmt = self
            .conn
            .prepare(
                "SELECT de.test_item_id, de.content_unit_id, de.origin, de.k_value
             FROM dependency_edges de
             JOIN reverse_index ri ON ri.content_unit_id = de.content_unit_id
             WHERE ri.content_unit_id = ?1 AND de.environment = ?2",
            )
            .map_err(|e| miette::miette!("Edge query prep failed: {}", e))?;

        for &cu_id in ctx.changed.iter().chain(ctx.unresolved.iter()) {
            let rows = edge_stmt
                .query_map(rusqlite::params![cu_id, ctx.current_environment], |row| {
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
        Ok(())
    }

    /// Load previously-failed tests (TIA-SAFE-007: always-run category 1).
    fn load_failed_tests(&self, ctx: &mut SelectionContext) -> miette::Result<()> {
        let mut ar_stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT test_item_id FROM run_history
             WHERE outcome = 'failed' AND environment = ?1 ORDER BY id DESC LIMIT 1000",
            )
            .map_err(|e| miette::miette!("Always-run query failed: {}", e))?;

        let rows = ar_stmt
            .query_map(rusqlite::params![ctx.current_environment], |row| {
                row.get::<_, u32>(0)
            })
            .map_err(|e| miette::miette!("Always-run exec failed: {}", e))?;
        for row in rows.flatten() {
            ctx.always_run.push(row);
        }
        Ok(())
    }

    /// Load tests with no recorded history (TIA-SAFE-007: no-history + newly-added).
    fn load_no_history_tests(&self, ctx: &mut SelectionContext) -> miette::Result<()> {
        let mut nh_stmt = self
            .conn
            .prepare(
                "SELECT ti.id FROM test_items ti
             LEFT JOIN run_history rh ON rh.test_item_id = ti.id AND rh.environment = ?1
             WHERE rh.id IS NULL",
            )
            .map_err(|e| miette::miette!("No-history query failed: {}", e))?;

        let nh_rows = nh_stmt
            .query_map(rusqlite::params![ctx.current_environment], |row| {
                row.get::<_, u32>(0)
            })
            .map_err(|e| miette::miette!("No-history exec failed: {}", e))?;
        for row in nh_rows.flatten() {
            ctx.always_run.push(row);
        }
        Ok(())
    }

    /// Load quarantined tests (TIA-SAFE-010: always-run category 4).
    fn load_quarantined_tests(&self, ctx: &mut SelectionContext) -> miette::Result<()> {
        let mut q_stmt = self
            .conn
            .prepare("SELECT id FROM test_items WHERE quarantined = 1")
            .map_err(|e| miette::miette!("Quarantine query failed: {}", e))?;

        let q_rows = q_stmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Quarantine exec failed: {}", e))?;
        for row in q_rows.flatten() {
            ctx.always_run.push(row);
            ctx.quarantined.push(row);
        }
        Ok(())
    }

    /// Compute invocation-level quality score (TIA-CONF-002).
    /// Combines four quality signals multiplicatively into a single ppm value.
    fn compute_invocation_quality(&self) -> u32 {
        let mut quality_score: f64 = 1.0;

        // 1. Coverage freshness — linearly decays from 1.0 (now) to 0.5 (72h stale)
        if let Ok(age_hours) = self.conn.query_row::<f64, _, _>(
            "SELECT COALESCE((julianday('now') - julianday(MAX(ingested_at))) * 24, 0.0) FROM run_history",
            [],
            |row| row.get(0),
        ) {
            if age_hours > 0.0 {
                let freshness = (1.0 - (age_hours / 144.0)).clamp(0.5, 1.0);
                quality_score *= freshness.max(0.1);
            }
        }

        // 2. Adapter resolution ratio — fraction of content units with real fingerprints
        if let (Ok(total), Ok(unknown)) = (
            self.conn
                .query_row("SELECT COUNT(*) FROM content_units", [], |row| {
                    row.get::<_, u32>(0)
                }),
            self.conn.query_row(
                "SELECT COUNT(*) FROM content_units WHERE fingerprint = 'unknown'",
                [],
                |row| row.get::<_, u32>(0),
            ),
        ) {
            if total > 0 {
                let ratio = (total - unknown) as f64 / total as f64;
                quality_score *= (0.5 + ratio * 0.5).max(0.1);
            }
        }

        // 3. History depth — average runs per test item, saturates at 5
        if let (Ok(test_count), Ok(run_count)) = (
            self.conn
                .query_row("SELECT COUNT(*) FROM test_items", [], |row| {
                    row.get::<_, u32>(0)
                }),
            self.conn
                .query_row("SELECT COUNT(*) FROM run_history", [], |row| {
                    row.get::<_, u32>(0)
                }),
        ) {
            if test_count > 0 {
                let avg = run_count as f64 / test_count as f64;
                let depth_score = (avg / 5.0).min(1.0);
                quality_score *= (0.5 + depth_score * 0.5).max(0.1);
            }
        }

        // 4. Environment match — compare current with most recent stored
        if let Ok(most_recent_env) = self.conn.query_row::<String, _, _>(
            "SELECT environment FROM run_history ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        ) {
            let current_env = "default".to_string();
            if most_recent_env != current_env {
                quality_score *= 0.5;
            }
        }

        quality_score = quality_score.clamp(0.1, 1.0);
        (quality_score * ONE as f64) as u32
    }

    /// Apply must-run rules (TIA-SAFE-009): check if changed files match
    /// must-run patterns and add resolved test IDs to always_run.
    fn apply_must_run_rules(&self, config: &Config, delta: &ChangeSet, ctx: &mut SelectionContext) {
        for (pattern_str, test_node_ids) in &config.must_run.rules {
            let Ok(pattern) = glob::Pattern::new(pattern_str) else {
                eprintln!("  ⚠️  Invalid must-run pattern: {}", pattern_str);
                continue;
            };
            for file in &delta.files {
                if pattern.matches(file) {
                    self.resolve_must_run_tests(test_node_ids, ctx);
                }
            }
        }
    }

    /// Resolve test node IDs to test_item IDs and add to always_run.
    fn resolve_must_run_tests(&self, test_node_ids: &[String], ctx: &mut SelectionContext) {
        for node_id in test_node_ids {
            if let Ok(tid) = self.conn.query_row(
                "SELECT id FROM test_items WHERE node_id = ?1 LIMIT 1",
                rusqlite::params![node_id],
                |row| row.get::<_, u32>(0),
            ) {
                if !ctx.always_run.contains(&tid) {
                    ctx.always_run.push(tid);
                }
            }
        }
    }

    /// Check and apply periodic full run (TIA-SAFE-006).
    fn check_periodic_full_run(
        &self,
        config: &Config,
        ctx: &mut SelectionContext,
    ) -> miette::Result<()> {
        if config.periodic_full_run.interval_hours == 0 {
            return Ok(());
        }

        let interval_secs = (config.periodic_full_run.interval_hours * 3600) as i64;
        let due: bool = self
            .conn
            .query_row(
                "SELECT (julianday('now') - julianday(MAX(ingested_at))) * 86400 >= ?1
                 FROM run_history",
                rusqlite::params![interval_secs],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(true);

        if due {
            let mut all_stmt = self
                .conn
                .prepare("SELECT id FROM test_items")
                .map_err(|e| miette::miette!("Failed to query test items: {}", e))?;
            let all_rows = all_stmt
                .query_map([], |row| row.get::<_, u32>(0))
                .map_err(|e| miette::miette!("Failed to exec test items query: {}", e))?;
            for row in all_rows.flatten() {
                if !ctx.always_run.contains(&row) {
                    ctx.always_run.push(row);
                }
            }
        }
        Ok(())
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

            // Resolve environment fingerprint from payload metadata (TIA-RUN-006)
            let env = results["environment"].as_object();
            let toolchain = env
                .and_then(|e| e.get("toolchain"))
                .and_then(|v| v.as_str());
            let os = env.and_then(|e| e.get("os")).and_then(|v| v.as_str());
            let environment = self.resolve_environment(toolchain, os)?;

            // Insert per-test results
            if let Some(tests) = results["tests"].as_array() {
                for test in tests {
                    let test_id = test["id"].as_u64().unwrap_or(0) as u32;
                    let outcome = test["outcome"].as_str().unwrap_or("passed");
                    let duration = test["duration_ms"].as_u64();
                    self.conn.execute(
                        "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![test_id, run_id, outcome, duration, environment],
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

            // Check for missed-selection incidents if this is a full run
            let is_full_run = results["full_run"].as_bool().unwrap_or(false);
            if is_full_run {
                if let Err(e) = self.detect_missed_selections(&run_id, results) {
                    eprintln!("  ⚠️  Missed-selection detection failed: {}", e);
                }
            }

            // Detect flaky tests from outcome oscillation (TIA-SAFE-011)
            if let Err(e) = self.detect_flaky_tests() {
                eprintln!("  ⚠️  Flaky detection failed: {}", e);
            }
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

    /// Count how many runs have been ingested.
    pub fn run_count(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM ingested_runs", [], |row| row.get(0))
    }

    /// Count how many tests are quarantined (flaky).
    pub fn quarantined_count(&self) -> rusqlite::Result<usize> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM test_items WHERE quarantined = 1",
            [],
            |row| row.get(0),
        )
    }

    /// Get the current schema version.
    pub fn schema_version(&self) -> rusqlite::Result<u32> {
        self.conn
            .query_row("SELECT version FROM _schema_version", [], |row| row.get(0))
    }

    /// Check that the store has been initialized, returning a human-readable error
    /// suggesting `testaruda init` if not (TIA-LOCAL-006).
    pub fn check_initialized(&self) -> miette::Result<()> {
        let tables_exist: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name=?1",
                [SCHEMA_TABLE],
                |row| row.get(0),
            )
            .map_err(|e| {
                miette::miette!(
                    "Failed to verify store state: {}. Try `testaruda init` to (re)initialize.",
                    e
                )
            })?;
        if !tables_exist || self.schema_version().is_err() {
            return Err(miette::miette!(
                "Store has not been initialized. Run `testaruda init` first to set up the local store."
            ));
        }
        Ok(())
    }

    /// Find the project root (git repo root or first ancestor with testaruda.toml).
    pub fn find_project_root() -> miette::Result<PathBuf> {
        find_project_root()
    }

    /// Store static dependency edges from an adapter (TIA-ADAPT-005).
    ///
    /// Creates content units for edges and inserts dependency edges into the
    /// dependency_edges and reverse_index tables.
    /// Normalize a DepEdge target path by stripping any trailing `:line_number` suffix.
    ///
    /// Runtime edges use `file:line` format (e.g., `src/Todo.jl:3`) for line-level
    /// precision, but content units are tracked at file-level granularity. Stripping
    /// the line suffix ensures edges from the same file map to the same content unit,
    /// which the selection engine looks up by file path.
    fn normalize_edge_target(path: &str) -> &str {
        if let Some(colon) = path.rfind(':') {
            // Check if the part after the last colon is a number (line number)
            let after = &path[colon + 1..];
            if after.parse::<u32>().is_ok() {
                return &path[..colon];
            }
        }
        path
    }

    pub fn store_static_deps(
        &self,
        adapter: &str,
        deps: &[crate::adapter::DepEdge],
    ) -> miette::Result<()> {
        // Group edges by normalized target path to avoid duplicate
        // content unit creation (SQLite UNIQUE treats NULL as distinct).
        let mut by_cu_path: std::collections::HashMap<String, Vec<&crate::adapter::DepEdge>> =
            std::collections::HashMap::new();

        for edge in deps {
            // Normalize target path: strip `:line` suffix from runtime edges
            // so file-level content units are used (TIA-ADAPT-008).
            let normalized = Self::normalize_edge_target(&edge.to);
            // Also make path relative to project root so it matches the format
            // used by the selection engine (delta.files are relative paths from git).
            let proj_root = self.project_root.to_string_lossy();
            let cu_path = if let Some(relative) = normalized.strip_prefix(&*proj_root) {
                relative.strip_prefix('/').unwrap_or(relative)
            } else {
                normalized
            };
            by_cu_path
                .entry(cu_path.to_string())
                .or_default()
                .push(edge);
        }

        for (cu_path, edges) in &by_cu_path {
            // Ensure the content unit (target) exists — once per unique path
            let cu_id = self
                .ensure_content_unit("default", cu_path, None, "source")
                .map_err(|e| miette::miette!("Failed to create content unit: {}", e))?;

            for edge in edges {
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
                         VALUES (?1, ?2, 'default', ?3, ?4)",
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

    /// Refresh all content unit fingerprints from disk.
    ///
    /// Walks every content unit in the store, recomputes its fingerprint
    /// from the current file on disk, and updates the stored value.
    /// Returns the number of units updated.
    pub fn refresh_fingerprints(&self) -> miette::Result<u32> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path FROM content_units")
            .map_err(|e| miette::miette!("Failed to prepare query: {}", e))?;

        let rows: Vec<(u32, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| miette::miette!("Failed to query content units: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let mut updated = 0u32;
        for (id, path) in &rows {
            let abs_path = if std::path::Path::new(path).is_absolute() {
                std::path::PathBuf::from(path)
            } else {
                self.project_root.join(path)
            };
            match Self::compute_fingerprint(&abs_path) {
                Ok(fp) => {
                    self.conn
                        .execute(
                            "UPDATE content_units SET fingerprint = ?1 WHERE id = ?2",
                            rusqlite::params![fp, id],
                        )
                        .map_err(|e| {
                            miette::miette!("Failed to update fingerprint for {}: {}", path, e)
                        })?;
                    updated += 1;
                }
                Err(_) => {
                    // File missing — leave fingerprint as-is (will show as
                    // "unresolved" on next select).
                }
            }
        }
        Ok(updated)
    }

    /// Export the dependency graph as JSON (TIA-STORE-003).
    ///
    /// Returns content units, test items, dependency edges, and run history
    /// in a documented interchange format suitable for import.
    pub fn export_graph(&self) -> miette::Result<serde_json::Value> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut tests = Vec::new();
        let mut runs = Vec::new();

        // Content units
        let mut stmt = self
            .conn
            .prepare("SELECT id, component, path, symbol, kind, fingerprint FROM content_units")
            .map_err(|e| miette::miette!("Graph query failed: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, u32>(0)?,
                    "component": row.get::<_, String>(1)?,
                    "path": row.get::<_, String>(2)?,
                    "symbol": row.get::<_, Option<String>>(3)?,
                    "kind": row.get::<_, String>(4)?,
                    "fingerprint": row.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| miette::miette!("Graph exec failed: {}", e))?;
        for row in rows.flatten() {
            nodes.push(row);
        }

        // Test items
        let mut tstmt = self
            .conn
            .prepare("SELECT id, component, adapter, node_id, quarantined FROM test_items")
            .map_err(|e| miette::miette!("Test items query failed: {}", e))?;
        let trows = tstmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, u32>(0)?,
                    "component": row.get::<_, String>(1)?,
                    "adapter": row.get::<_, String>(2)?,
                    "node_id": row.get::<_, String>(3)?,
                    "quarantined": row.get::<_, i32>(4)? != 0,
                }))
            })
            .map_err(|e| miette::miette!("Test items exec failed: {}", e))?;
        for row in trows.flatten() {
            tests.push(row);
        }

        // Dependency edges
        let mut estmt = self
            .conn
            .prepare(
                "SELECT de.test_item_id, de.content_unit_id, de.environment, de.origin, de.k_value,
                        ti.node_id, cu.path
                 FROM dependency_edges de
                 JOIN test_items ti ON ti.id = de.test_item_id
                 JOIN content_units cu ON cu.id = de.content_unit_id",
            )
            .map_err(|e| miette::miette!("Edge export failed: {}", e))?;
        let erows = estmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "from": row.get::<_, u32>(0)?,
                    "to": row.get::<_, u32>(1)?,
                    "environment": row.get::<_, String>(2)?,
                    "origin": row.get::<_, String>(3)?,
                    "k": row.get::<_, u32>(4)?,
                    "from_node_id": row.get::<_, String>(5)?,
                    "to_path": row.get::<_, String>(6)?,
                }))
            })
            .map_err(|e| miette::miette!("Edge export exec failed: {}", e))?;
        for row in erows.flatten() {
            edges.push(row);
        }

        // Run history
        let mut rstmt = self
            .conn
            .prepare(
                "SELECT rh.test_item_id, rh.run_id, rh.outcome, rh.duration_ms, rh.environment, ti.node_id
                 FROM run_history rh
                 JOIN test_items ti ON ti.id = rh.test_item_id",
            )
            .map_err(|e| miette::miette!("Run history query failed: {}", e))?;
        let rrows = rstmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "test_item_id": row.get::<_, u32>(0)?,
                    "run_id": row.get::<_, String>(1)?,
                    "outcome": row.get::<_, String>(2)?,
                    "duration_ms": row.get::<_, Option<u64>>(3)?,
                    "environment": row.get::<_, String>(4)?,
                    "node_id": row.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| miette::miette!("Run history exec failed: {}", e))?;
        for row in rrows.flatten() {
            runs.push(row);
        }

        Ok(serde_json::json!({
            "format": "testaruda-graph-v1",
            "content_units": nodes,
            "test_items": tests,
            "edges": edges,
            "run_history": runs,
        }))
    }

    /// Import a dependency graph from a JSON export (TIA-STORE-003).
    ///
    /// Reconstructs content units, test items, dependency edges, and run
    /// history from the exported interchange format.
    pub fn import_graph(&self, graph: &serde_json::Value) -> miette::Result<()> {
        // Verify format
        let format = graph["format"].as_str().unwrap_or("");
        if !format.starts_with("testaruda-graph-v") {
            return Err(miette::miette!(
                "Unknown graph format: '{}'. Expected 'testaruda-graph-v1'",
                format
            ));
        }

        self.import_graph_content_units(graph)?;
        self.import_graph_test_items(graph)?;
        self.import_graph_edges(graph)?;
        self.import_graph_run_history(graph)?;

        Ok(())
    }

    /// Import content units from a graph export.
    fn import_graph_content_units(&self, graph: &serde_json::Value) -> miette::Result<()> {
        if let Some(units) = graph["content_units"].as_array() {
            for unit in units {
                let component = unit["component"].as_str().unwrap_or("default");
                let path = unit["path"].as_str().unwrap_or("");
                let symbol = unit["symbol"].as_str();
                let kind = unit["kind"].as_str().unwrap_or("source");
                let fingerprint = unit["fingerprint"].as_str().unwrap_or("unknown");
                self.ensure_content_unit(component, path, symbol, kind)
                    .map_err(|e| miette::miette!("Failed to import content unit: {}", e))?;
                self.conn.execute(
                    "UPDATE content_units SET fingerprint = ?1 WHERE component = ?2 AND path = ?3",
                    rusqlite::params![fingerprint, component, path],
                ).map_err(|e| miette::miette!("Failed to set fingerprint: {}", e))?;
            }
        }
        Ok(())
    }

    /// Import test items from a graph export.
    fn import_graph_test_items(&self, graph: &serde_json::Value) -> miette::Result<()> {
        if let Some(items) = graph["test_items"].as_array() {
            for item in items {
                let component = item["component"].as_str().unwrap_or("default");
                let adapter = item["adapter"].as_str().unwrap_or("import");
                let node_id = item["node_id"].as_str().unwrap_or("");
                let quarantined = item["quarantined"].as_bool().unwrap_or(false);
                let quarantined_int = if quarantined { 1 } else { 0 };
                self.conn.execute(
                    "INSERT OR IGNORE INTO test_items (component, adapter, node_id, quarantined)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![component, adapter, node_id, quarantined_int],
                ).map_err(|e| miette::miette!("Failed to import test item: {}", e))?;
            }
        }
        Ok(())
    }

    /// Import dependency edges from a graph export.
    fn import_graph_edges(&self, graph: &serde_json::Value) -> miette::Result<()> {
        if let Some(edges) = graph["edges"].as_array() {
            for edge in edges {
                let from_node_id = edge["from_node_id"].as_str();
                let to_path = edge["to_path"].as_str();
                let environment = edge["environment"].as_str().unwrap_or("default");
                let origin = edge["origin"].as_str().unwrap_or("static");
                let k = edge["k"].as_u64().unwrap_or(1000000) as u32;

                let from = self.resolve_edge_from(from_node_id)?;
                let to = self.resolve_edge_to(to_path)?;

                self.conn.execute(
                    "INSERT OR IGNORE INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![from, to, environment, origin, k],
                ).map_err(|e| miette::miette!("Failed to import edge: {}", e))?;
                self.conn.execute(
                    "INSERT OR IGNORE INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
                    rusqlite::params![to, from],
                ).map_err(|e| miette::miette!("Failed to update reverse index: {}", e))?;
            }
        }
        Ok(())
    }

    /// Resolve a test_item_id from a node_id string.
    fn resolve_edge_from(&self, from_node_id: Option<&str>) -> miette::Result<u32> {
        match from_node_id {
            Some(nid) => self
                .conn
                .query_row::<u32, _, _>(
                    "SELECT id FROM test_items WHERE node_id = ?1 LIMIT 1",
                    rusqlite::params![nid],
                    |row| row.get(0),
                )
                .map_err(|e| miette::miette!("Failed to resolve test '{}': {}", nid, e)),
            None => Err(miette::miette!("Edge missing 'from_node_id'")),
        }
    }

    /// Resolve a content_unit_id from a path string.
    fn resolve_edge_to(&self, to_path: Option<&str>) -> miette::Result<u32> {
        match to_path {
            Some(p) => self
                .conn
                .query_row::<u32, _, _>(
                    "SELECT id FROM content_units WHERE path = ?1 LIMIT 1",
                    rusqlite::params![p],
                    |row| row.get(0),
                )
                .map_err(|e| miette::miette!("Failed to resolve content unit '{}': {}", p, e)),
            None => Err(miette::miette!("Edge missing 'to_path'")),
        }
    }

    /// Import run history from a graph export.
    fn import_graph_run_history(&self, graph: &serde_json::Value) -> miette::Result<()> {
        if let Some(runs) = graph["run_history"].as_array() {
            for run in runs {
                let node_id = run["node_id"].as_str();
                let run_id = run["run_id"].as_str().unwrap_or("");
                let outcome = run["outcome"].as_str().unwrap_or("passed");
                let duration_ms = run["duration_ms"].as_u64();
                let environment = run["environment"].as_str().unwrap_or("default");

                let test_item_id = match node_id {
                    Some(nid) => self
                        .conn
                        .query_row::<u32, _, _>(
                            "SELECT id FROM test_items WHERE node_id = ?1 LIMIT 1",
                            rusqlite::params![nid],
                            |row| row.get(0),
                        )
                        .map_err(|e| {
                            miette::miette!(
                                "Failed to resolve test '{}' for run history: {}",
                                nid,
                                e
                            )
                        })?,
                    None => continue,
                };

                self.conn.execute(
                    "INSERT OR IGNORE INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![test_item_id, run_id, outcome, duration_ms, environment],
                ).map_err(|e| miette::miette!("Failed to import run history: {}", e))?;
            }
        }
        Ok(())
    }

    /// Generate a Soufflé Datalog program from the current store (TIA-ENG-010).
    ///
    /// The generated program mirrors the Ascent selection rules and can be
    /// evaluated by the Soufflé CLI for cross-validation.
    pub fn generate_datalog(&self) -> miette::Result<String> {
        let mut prog = String::new();

        // Declare relations
        prog.push_str("// Auto-generated from testaruda store\n");
        prog.push_str(".decl changed(cu: number)\n");
        prog.push_str(".decl unresolved(cu: number)\n");
        prog.push_str(".decl cu_dep(a: number, b: number, origin: symbol, weight: number)\n");
        prog.push_str(".decl test_dep(t: number, cu: number, origin: symbol, weight: number)\n");
        prog.push_str(".decl always_run(t: number)\n");
        prog.push_str(".decl comp_fallback(c: number)\n");
        prog.push_str(".decl test_comp(t: number, c: number)\n");
        prog.push_str(".decl impacted(cu: number)\n");
        prog.push_str(".decl affected(t: number, conf: number)\n");
        prog.push_str(".decl output_affected(t: number)\n\n");

        // Changed content units
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM content_units WHERE fingerprint != 'unknown' LIMIT 5")
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        for row in rows.flatten() {
            prog.push_str(&format!("changed({}).\n", row));
        }

        // Dependency edges
        let mut estmt = self
            .conn
            .prepare("SELECT test_item_id, content_unit_id, origin, k_value FROM dependency_edges WHERE environment = 'default'")
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        let erows = estmt
            .query_map([], |row| {
                let tid: u32 = row.get(0)?;
                let cu_id: u32 = row.get(1)?;
                let origin: String = row.get(2)?;
                let k: u32 = row.get(3)?;
                Ok((tid, cu_id, origin, k))
            })
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        for row in erows.flatten() {
            prog.push_str(&format!(
                "test_dep({}, {}, \"{}\", {}).\n",
                row.0, row.1, row.2, row.3
            ));
        }

        // Always-run tests
        let mut astmt = self
            .conn
            .prepare("SELECT DISTINCT test_item_id FROM run_history WHERE outcome = 'failed'")
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        let arows = astmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Query failed: {}", e))?;
        for row in arows.flatten() {
            prog.push_str(&format!("always_run({}).\n", row));
        }

        // Rules
        prog.push_str("\n// Selection rules\n");
        prog.push_str("impacted(cu) :- changed(cu).\n");
        prog.push_str("impacted(cu) :- unresolved(cu).\n");
        prog.push_str("impacted(a) :- cu_dep(a, b, _, _), impacted(b).\n");
        prog.push_str("affected(t, 1000000) :- test_dep(t, cu, _, _), impacted(cu).\n");
        prog.push_str("affected(t, 1000000) :- always_run(t).\n");
        prog.push_str("affected(t, 1000000) :- comp_fallback(k), test_comp(t, k).\n");
        prog.push_str("output_affected(t) :- affected(t, _).\n");
        prog.push_str("\n.output output_affected\n");

        Ok(prog)
    }

    /// Explain why a test was or was not selected.
    pub fn explain(
        &self,
        test_id: &str,
        _change: Option<&str>,
    ) -> miette::Result<serde_json::Value> {
        let tid: u32 = test_id.parse().map_err(|_| {
            miette::miette!(
                "Invalid test ID '{}' — expected a numeric identifier",
                test_id
            )
        })?;

        // Verify the test item exists
        let exists: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM test_items WHERE id = ?1",
                [tid],
                |row| row.get(0),
            )
            .map_err(|e| miette::miette!("Explain query failed: {}", e))?;

        if !exists {
            return Err(miette::miette!(
                "Test ID '{}' not found in the store. Use 'testaruda metrics' to list known test IDs.",
                tid
            ));
        }
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
        self.conn
            .execute(
                "INSERT OR REPLACE INTO selection_cache (fingerprint, selection_json, cached_at)
             VALUES (?1, ?2, datetime('now'))",
                rusqlite::params![fingerprint, selection_json],
            )
            .map_err(|e| miette::miette!("Failed to cache selection: {}", e))?;
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

    /// Get the path for a content unit by ID.
    pub fn get_content_unit_path(&self, id: u32) -> miette::Result<String> {
        self.conn
            .query_row(
                "SELECT path FROM content_units WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| miette::miette!("Failed to get content unit path: {}", e))
    }

    /// Check if any test has a dependency edge to the given content unit.
    pub fn has_test_for_content_unit(&self, cu_id: u32) -> miette::Result<bool> {
        let exists: bool = self
            .conn
            .query_row(
                "SELECT 1 FROM dependency_edges WHERE content_unit_id = ?1 LIMIT 1",
                rusqlite::params![cu_id],
                |_| Ok(true),
            )
            .unwrap_or(false);
        Ok(exists)
    }

    /// Get the content unit info (path, symbol, kind) for a given ID.
    pub fn get_content_unit_info(
        &self,
        id: u32,
    ) -> miette::Result<(String, Option<String>, String)> {
        self.conn
            .query_row(
                "SELECT path, symbol, kind FROM content_units WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    let path: String = row.get(0)?;
                    let symbol: Option<String> = row.get(1)?;
                    let kind: String = row.get(2)?;
                    Ok((path, symbol, kind))
                },
            )
            .map_err(|e| miette::miette!("Failed to get content unit info: {}", e))
    }

    /// Get the node_id for a test item by its internal ID.
    pub fn get_test_node_id(&self, id: u32) -> miette::Result<String> {
        self.conn
            .query_row(
                "SELECT node_id FROM test_items WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| miette::miette!("Failed to get test node ID: {}", e))
    }

    /// Look up a test item ID by its node_id string.
    pub fn lookup_test_item_id(&self, node_id: &str) -> miette::Result<u32> {
        self.conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id = ?1",
                rusqlite::params![node_id],
                |row| row.get::<_, u32>(0),
            )
            .map_err(|e| miette::miette!("Failed to look up test item '{}': {}", node_id, e))
    }

    /// Look up a content unit ID by component and path.
    pub fn lookup_content_unit(
        &self,
        component: &str,
        path: &str,
    ) -> miette::Result<(u32, String)> {
        self.conn
            .query_row(
                "SELECT id, fingerprint FROM content_units WHERE component = ?1 AND path = ?2",
                rusqlite::params![component, path],
                |row| {
                    let id: u32 = row.get(0)?;
                    let fingerprint: String = row.get(1)?;
                    Ok((id, fingerprint))
                },
            )
            .map_err(|e| miette::miette!("Failed to look up content unit: {}", e))
    }

    /// Get all test item IDs that have dependency edges to the given content units.
    pub fn get_test_ids_for_content_units(
        &self,
        changed: &[u32],
        unresolved: &[u32],
    ) -> miette::Result<Vec<u32>> {
        let mut ids = Vec::new();
        let all_cus: Vec<u32> = changed.iter().chain(unresolved.iter()).copied().collect();
        if all_cus.is_empty() {
            return Ok(ids);
        }

        // Build a parameterized query with placeholders
        let placeholders: Vec<String> = all_cus.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT DISTINCT test_item_id FROM dependency_edges
             WHERE content_unit_id IN ({})
             ORDER BY test_item_id",
            placeholders.join(",")
        );

        let params: Vec<&dyn rusqlite::types::ToSql> = all_cus
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| miette::miette!("Query prep failed: {}", e))?;

        let rows = stmt
            .query_map(params.as_slice(), |row| row.get::<_, u32>(0))
            .map_err(|e| miette::miette!("Query failed: {}", e))?;

        for row in rows.flatten() {
            ids.push(row);
        }
        Ok(ids)
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
                 WHERE duration_ms IS NOT NULL AND environment = 'default'
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

    /// Load the historical failure rate (failed / total) for each test item.
    ///
    /// Returns a map of test_item_id → failure_rate (0.0 to 1.0).
    /// Tests with no run history are omitted from the map.
    ///
    /// Used by predictive ranking (TIA-SEL-007).
    pub fn load_failure_rates(&self) -> miette::Result<std::collections::HashMap<u32, f64>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT test_item_id,
                        CAST(SUM(CASE WHEN outcome = 'failed' THEN 1 ELSE 0 END) AS REAL) /
                        CAST(COUNT(*) AS REAL) AS failure_rate
                 FROM run_history
                 WHERE environment = 'default'
                 GROUP BY test_item_id",
            )
            .map_err(|e| miette::miette!("Failure rate query prep failed: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                let id: u32 = row.get(0)?;
                let rate: f64 = row.get(1)?;
                Ok((id, rate))
            })
            .map_err(|e| miette::miette!("Failure rate query failed: {}", e))?;
        let mut map = std::collections::HashMap::new();
        for row in rows.flatten() {
            map.insert(row.0, row.1);
        }
        Ok(map)
    }

    /// Evaluate the predictive ranking model against held-out run history.
    ///
    /// Uses the most recent run as the hold-out test set, and all previous runs
    /// as training data. Computes recall@k where k = number of tests in the
    /// hold-out set.
    ///
    /// Returns zeroed metrics if there is insufficient history for a hold-out
    /// split (fewer than 2 distinct runs or no test items in hold-out).
    ///
    /// Used by predictive ranking calibration gate (TIA-VER-005).
    pub fn evaluate_ranking_calibration(&self) -> miette::Result<CalibrationMetrics> {
        // Get all distinct run_ids in chronological order
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT run_id FROM run_history
                 WHERE environment = 'default'
                 ORDER BY id ASC",
            )
            .map_err(|e| miette::miette!("Run ID query prep failed: {}", e))?;
        let run_ids: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| miette::miette!("Run ID query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        // Need at least 2 runs for a train/test split
        if run_ids.len() < 2 {
            return Ok(CalibrationMetrics {
                total_test_items: 0,
                total_failures: 0,
                captured_failures: 0,
                recall_at_k: 0.0,
                k: 0,
            });
        }

        let test_run_id = &run_ids[run_ids.len() - 1];

        // Compute failure rates from training data (all runs except the last)
        let mut stmt = self
            .conn
            .prepare(
                "SELECT test_item_id,
                        CAST(SUM(CASE WHEN outcome = 'failed' THEN 1 ELSE 0 END) AS REAL) /
                        CAST(COUNT(*) AS REAL) AS failure_rate
                 FROM run_history
                 WHERE environment = 'default' AND run_id != ?1
                 GROUP BY test_item_id",
            )
            .map_err(|e| miette::miette!("Training query prep failed: {}", e))?;
        let training_rows: Vec<(u32, f64)> = stmt
            .query_map(rusqlite::params![test_run_id], |row| {
                let id: u32 = row.get(0)?;
                let rate: f64 = row.get(1)?;
                Ok((id, rate))
            })
            .map_err(|e| miette::miette!("Training query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        // Build predicted ranking: sort by descending failure rate, ID tiebreaker
        let mut predicted: Vec<(u32, f64)> = training_rows;
        predicted.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Get hold-out test outcomes
        let mut stmt = self
            .conn
            .prepare(
                "SELECT test_item_id, outcome FROM run_history
                 WHERE environment = 'default' AND run_id = ?1",
            )
            .map_err(|e| miette::miette!("Test query prep failed: {}", e))?;
        let test_rows: Vec<(u32, String)> = stmt
            .query_map(rusqlite::params![test_run_id], |row| {
                let id: u32 = row.get(0)?;
                let outcome: String = row.get(1)?;
                Ok((id, outcome))
            })
            .map_err(|e| miette::miette!("Test query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        let total_test_items = test_rows.len();
        let total_failures = test_rows
            .iter()
            .filter(|(_, outcome)| outcome == "failed")
            .count();

        if total_test_items == 0 || total_failures == 0 {
            return Ok(CalibrationMetrics {
                total_test_items,
                total_failures,
                captured_failures: 0,
                recall_at_k: 0.0,
                k: total_test_items,
            });
        }

        // Build predicted rank map: test_item_id → rank position (0-indexed)
        let rank_map: std::collections::HashMap<u32, usize> = predicted
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i))
            .collect();

        // Tests not in training data get rank = predicted.len() (after all trained)
        let k = total_test_items;
        let captured_failures = test_rows
            .iter()
            .filter(|(id, outcome)| {
                if outcome != "failed" {
                    return false;
                }
                let rank = rank_map.get(id).copied().unwrap_or(predicted.len());
                rank < k
            })
            .count();

        let recall_at_k = if total_failures > 0 {
            captured_failures as f64 / total_failures as f64
        } else {
            0.0
        };

        Ok(CalibrationMetrics {
            total_test_items,
            total_failures,
            captured_failures,
            recall_at_k,
            k,
        })
    }

    /// Resolve or create an environment fingerprint (TIA-RUN-006, TIA-CORE-008).
    ///
    /// Looks up the environment_fingerprints table by toolchain and OS.
    /// If not found, inserts a new fingerprint. Returns the fingerprint string.
    pub fn resolve_environment(
        &self,
        toolchain: Option<&str>,
        os: Option<&str>,
    ) -> miette::Result<String> {
        // First try exact match
        if let (Some(tc), Some(os_val)) = (toolchain, os) {
            if let Ok(fp) = self.conn.query_row(
                "SELECT fingerprint FROM environment_fingerprints
                 WHERE toolchain = ?1 AND os = ?2",
                rusqlite::params![tc, os_val],
                |row| row.get::<_, String>(0),
            ) {
                return Ok(fp);
            }
        }

        // Compute a deterministic fingerprint from metadata
        use std::hash::{Hash, Hasher};
        let fingerprint = match (toolchain, os) {
            (Some(tc), Some(os_val)) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                format!("tc:{}|os:{}", tc, os_val).hash(&mut hasher);
                format!("env-{:016x}", hasher.finish())
            }
            _ => "default".to_string(),
        };

        // Insert if not exists
        self.conn
            .execute(
                "INSERT OR IGNORE INTO environment_fingerprints (fingerprint, toolchain, os)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![fingerprint, toolchain, os],
            )
            .map_err(|e| miette::miette!("Failed to store environment fingerprint: {}", e))?;

        Ok(fingerprint)
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
        assert_eq!(
            version, SCHEMA_VERSION,
            "fresh store should have current version"
        );
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
            assert_eq!(
                version, SCHEMA_VERSION,
                "older schema should be migrated to current"
            );
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
                .execute(&format!("DELETE FROM {}", SCHEMA_TABLE), [])
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

    #[allow(clippy::assertions_on_constants)]
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

        store.set_cached_selection("fp1", r#"{"sel":1}"#).unwrap();
        store.set_cached_selection("fp2", r#"{"sel":2}"#).unwrap();

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
        store.set_cached_selection("fp1", r#"{"sel":99}"#).unwrap();
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

        store.set_cached_selection("fp_a", r#"{}"#).unwrap();
        store.set_cached_selection("fp_b", r#"{}"#).unwrap();

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
        assert!(
            store.ingest(&payload).is_ok(),
            "first ingest should succeed"
        );

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

    // ── Always-run set completeness tests (TIA-SAFE-007, TIA-SAFE-010) ──

    #[test]
    fn test_always_run_includes_previously_failed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test','failed_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'r1', 'failed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert!(
            ctx.always_run.contains(&tid),
            "previously-failed test should be always-run"
        );
    }

    #[test]
    fn test_always_run_includes_no_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'no_history_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert!(
            ctx.always_run.contains(&tid),
            "test with no run history should be always-run"
        );
    }

    #[test]
    fn test_always_run_includes_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id, quarantined)
             VALUES ('default', 'test', 'quarantined_test', 1)",
            [],
        )
        .unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert!(
            ctx.always_run.contains(&tid),
            "quarantined test should be always-run"
        );
        assert!(
            ctx.quarantined.contains(&tid),
            "quarantined test should appear in quarantined set"
        );
    }

    #[test]
    fn test_detect_flaky_tests_oscillation() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'flaky_test')",
            [],
        )
        .unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Insert oscillating outcomes (passed, failed) across recent runs
        for (run_id, outcome) in &[("run1", "passed"), ("run2", "failed")] {
            conn.execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, ?2, ?3, 100, 'default')",
                rusqlite::params![tid, run_id, outcome],
            )
            .unwrap();
        }

        store.detect_flaky_tests().unwrap();

        let quarantined: i32 = conn
            .query_row(
                "SELECT quarantined FROM test_items WHERE id = ?1",
                rusqlite::params![tid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(quarantined, 1, "flaky test should be quarantined");
    }

    #[test]
    fn test_detect_flaky_tests_stable_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'stable_test')",
            [],
        )
        .unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Insert consistent outcomes (all passed)
        for (i, outcome) in (0..3).map(|i| (i, "passed")) {
            conn.execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, ?2, ?3, 100, 'default')",
                rusqlite::params![tid, format!("run{}", i), outcome],
            )
            .unwrap();
        }

        store.detect_flaky_tests().unwrap();

        let quarantined: i32 = conn
            .query_row(
                "SELECT quarantined FROM test_items WHERE id = ?1",
                rusqlite::params![tid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(quarantined, 0, "stable test should not be quarantined");
    }

    #[test]
    fn test_always_run_all_categories_in_selection() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let (_cu_id, tid_failed, tid_nohist, tid_quar, tid_passed) =
            setup_always_run_fixture(&dir, store.conn());

        // Change the file to trigger selection
        let file_path = dir.path().join("src/lib.rs");
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec![file_path.to_string_lossy().to_string()],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();
        let ids: std::collections::HashSet<u32> = sel.tests.iter().map(|t| t.id).collect();

        assert!(
            ids.contains(&tid_failed),
            "previously-failed should be selected"
        );
        assert!(ids.contains(&tid_nohist), "no-history should be selected");
        assert!(ids.contains(&tid_quar), "quarantined should be selected");
        assert!(
            !ids.contains(&tid_passed),
            "passed-with-history should NOT be selected"
        );
    }

    /// Set up test fixture for always-run categories.
    /// Returns (cu_id, tid_failed, tid_nohist, tid_quar, tid_passed).
    fn setup_always_run_fixture(
        dir: &tempfile::TempDir,
        conn: &rusqlite::Connection,
    ) -> (u32, u32, u32, u32, u32) {
        // Insert content unit so fingerprint matching works
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', ?1, NULL, 'source', ?2)",
            rusqlite::params![file_path.to_string_lossy().to_string(), fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // cat 1: previously-failed
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'failed')",
            [],
        ).unwrap();
        let tid_failed: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='failed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'fail-run', 'failed', 10, 'default')",
            rusqlite::params![tid_failed],
        )
        .unwrap();

        // cat 2: newly-added / no-history
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'no_history')",
            [],
        ).unwrap();
        let tid_nohist: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='no_history'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // cat 3: quarantined
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id, quarantined)
             VALUES ('default', 'test', 'quarantined', 1)",
            [],
        )
        .unwrap();
        let tid_quar: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='quarantined'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // cat 4: passed-with-history (should NOT be always-run)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'passed')",
            [],
        ).unwrap();
        let tid_passed: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='passed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'pass-run', 'passed', 10, 'default')",
            rusqlite::params![tid_passed],
        )
        .unwrap();

        // Wire a small dep graph so selection can run through Engine
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid_failed],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid_failed, cu_id],
        ).unwrap();

        (cu_id, tid_failed, tid_nohist, tid_quar, tid_passed)
    }

    #[test]
    fn test_quarantined_flag_in_selection_result() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Content unit
        let file_path = dir.path().join("src/main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', ?1, NULL, 'source', ?2)",
            rusqlite::params![file_path.to_string_lossy().to_string(), fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // A quarantined test
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id, quarantined)
             VALUES ('default', 'test', 'quarantined_test', 1)",
            [],
        )
        .unwrap();
        let tid_quar: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // A regular always-run test (previously failed)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'failed_test')",
            [],
        ).unwrap();
        let tid_fail: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='failed_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'fail-r', 'failed', 10, 'default')",
            rusqlite::params![tid_fail],
        )
        .unwrap();

        // Wire both to the content unit (so they'd be selected even without always_run)
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid_quar],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid_quar, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid_fail],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid_fail, cu_id],
        ).unwrap();

        // Change the file
        std::fs::write(&file_path, b"fn main() { println!(\"hi\"); }").unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec![file_path.to_string_lossy().to_string()],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();

        let quarantined_test = sel.tests.iter().find(|t| t.id == tid_quar).unwrap();
        assert!(
            quarantined_test.quarantined,
            "quarantined test should have quarantined=true"
        );

        let failed_test = sel.tests.iter().find(|t| t.id == tid_fail).unwrap();
        assert!(
            !failed_test.quarantined,
            "previously-failed test should have quarantined=false"
        );
    }

    #[test]
    fn test_no_history_test_not_quarantined() {
        // A test with no run history is always-run but NOT quarantined
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'fresh')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert!(
            ctx.always_run.contains(&tid),
            "no-history should be always-run"
        );
        assert!(
            !ctx.quarantined.contains(&tid),
            "no-history test should NOT be quarantined"
        );
    }

    #[test]
    fn test_schema_migration_v1_to_v2() {
        let dir = tempfile::tempdir().unwrap();
        let store_dir = dir.path().join("store_v1");

        // Create a v1 database manually
        std::fs::create_dir_all(&store_dir).unwrap();
        let db_path = store_dir.join("store.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS test_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                component TEXT NOT NULL,
                adapter TEXT NOT NULL,
                node_id TEXT NOT NULL,
                UNIQUE(component, adapter, node_id)
            );
            INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'v1_test');
            CREATE TABLE IF NOT EXISTS _schema_version (version INTEGER NOT NULL);
            INSERT INTO _schema_version (version) VALUES (1);",
        ).unwrap();
        drop(conn);

        // Open with Store (should trigger migration)
        let store = Store::open(store_dir.clone()).unwrap();
        store.initialize().unwrap();

        // Verify quarantined column exists and defaults to 0
        let quarantined: i32 = store
            .conn()
            .query_row(
                "SELECT quarantined FROM test_items WHERE node_id='v1_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(quarantined, 0, "migrated v1 test should have quarantined=0");

        // Verify schema version is 2
        let version: u32 = store
            .conn()
            .query_row("SELECT MAX(version) FROM _schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 4, "schema should be migrated to v4");
    }

    // ── Content unit uniqueness (testaruda-p37i) ──

    #[test]
    fn test_content_unit_dedup_when_symbol_is_null() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Call ensure_content_unit twice with the same (component, path, None)
        let id1 = store
            .ensure_content_unit("test-comp", "src/lib.rs", None, "source")
            .unwrap();
        let id2 = store
            .ensure_content_unit("test-comp", "src/lib.rs", None, "source")
            .unwrap();

        // Both calls should return the same id
        assert_eq!(
            id1, id2,
            "ensure_content_unit should return same id for duplicate (NULL symbol)"
        );

        // Verify only one row in the table
        let count: u32 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM content_units WHERE component = ?1 AND path = ?2",
                rusqlite::params!["test-comp", "src/lib.rs"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 1,
            "content_units should have exactly 1 row for (test-comp, src/lib.rs, NULL)"
        );
    }

    // ── Confidence floor and gating tests (TIA-CONF-002, TIA-SAFE-002, TIA-SAFE-003) ──

    #[test]
    fn test_invocation_quality_defaults_to_one() {
        // An empty store should have invocation_quality = ONE (no adjustment)
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert_eq!(
            ctx.invocation_quality, ONE,
            "empty store should have max invocation quality"
        );
    }

    #[test]
    fn test_confidence_threshold_defaults_to_500000() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        // Write empty config to project root (parent of store dir)
        std::fs::write(dir.path().join("testaruda.toml"), "").unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert_eq!(
            ctx.confidence_threshold,
            ONE / 2,
            "default confidence threshold should be 0.5 (500,000 ppm)"
        );
    }

    #[test]
    fn test_invocation_quality_reflects_adapter_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Insert a content unit with real fingerprint
        let file_path = dir.path().join("src/main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'resolved.rs', NULL, 'source', ?1)",
            rusqlite::params![fp],
        )
        .unwrap();

        // Insert an unresolved content unit
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'unknown.rs', NULL, 'source', 'unknown')",
            [],
        )
        .unwrap();

        // Insert a test item and run history (to avoid freshness penalty)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 't1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (1, 'r1', 'passed', 10, 'default')",
            [],
        )
        .unwrap();

        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();

        // 50% resolution → quality should be reduced but > 0.1
        assert!(
            ctx.invocation_quality < ONE,
            "low resolution should reduce quality"
        );
        assert!(
            ctx.invocation_quality > ONE / 10,
            "quality should not drop below 0.1"
        );
    }

    #[test]
    fn test_confidence_floor_triggers_component_fallback() {
        // Set up a graph with very low edge weights, triggering fallback
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"confidence_threshold = 0.9"#,
        )
        .unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Content unit
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', ?1)",
            rusqlite::params![fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Test item in component 1, with low edge weight (100,000 = 0.1)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'low_conf_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Wire dependency with low weight
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 100000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();

        // Give test a run history so it's not always-run
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Change the file to trigger selection
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec!["src/lib.rs".to_string()],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();

        // Test should be selected (via test_dep) even though threshold is 0.9
        // The edge weight is 0.1, quality is 1.0 (fresh store), so effective
        // confidence = 0.1. Since 0.1 < 0.9 threshold, component fallback should
        // trigger, selecting all tests in the component.
        let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
        assert!(ids.contains(&tid), "test should be selected via fallback");
    }

    #[test]
    fn test_high_confidence_avoids_fallback() {
        // Set up a graph with high edge weights, no fallback needed
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        // Write config to the project root (parent of store dir)
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"confidence_threshold = 0.3"#,
        )
        .unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Content unit
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', ?1)",
            rusqlite::params![fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Test item with high edge weight (900,000 = 0.9)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'high_conf_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 900000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();

        // Give test a run history
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Change the file
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec!["src/lib.rs".to_string()],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();

        // Test should be selected via the dependency path (not fallback)
        let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
        assert!(ids.contains(&tid), "test should be selected via dep path");
        // Confidence should be 0.9 * 0.6 (history depth quality) = 0.54
        let test = sel.tests.iter().find(|t| t.id == tid).unwrap();
        assert!(
            (test.confidence - 0.54).abs() < 0.01,
            "confidence should be ~0.54 (0.9 edge * 0.6 quality), got {}",
            test.confidence
        );
    }

    #[test]
    fn test_always_run_confidence_immune_to_quality() {
        // Always-run tests should always have confidence = 1.0 regardless
        // of invocation quality (TIA-CONF-002: always-run is force-select,
        // not evidence-based).
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Insert a large number of unresolved content units to drive quality down
        for i in 0..20 {
            conn.execute(
                "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
                 VALUES ('default', ?1, NULL, 'source', 'unknown')",
                rusqlite::params![format!("unknown_{}.rs", i)],
            )
            .unwrap();
        }

        // A test item (no history → always-run)
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'always_run_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec![],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();

        let test = sel.tests.iter().find(|t| t.id == tid).unwrap();
        assert_eq!(
            test.confidence, 1.0,
            "always-run test should have confidence 1.0 regardless of quality"
        );
    }

    // ── Missed-selection incident tests (TIA-SAFE-008) ──

    #[test]
    fn test_detect_missed_selections_no_previous_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let results = serde_json::json!({
            "run_id": "full-run-1",
            "full_run": true,
            "tests": [{"id": 1, "outcome": "failed", "duration_ms": 10}]
        });
        let incidents = store
            .detect_missed_selections("full-run-1", &results)
            .unwrap();
        assert_eq!(incidents, 0, "no previous provenance → no incidents");
    }

    #[test]
    fn test_detect_missed_selections_with_skipped_failure() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Set up content unit and test items
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', 'abc123')",
            [],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_a')",
            [],
        ).unwrap();
        let test_id: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Give test a run history so it's not always-run
        // (Intentionally no static dep edge — test won't be selected via
        // dependency path. The missed-selection incident will create a
        // manual edge instead.)
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![test_id],
        )
        .unwrap();

        // Simulate a prior selection run:
        // - test_a was NOT selected (selected=0) — so it was skipped
        // - Another test was selected with a witness including cu_id
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'other_test')",
            [],
        ).unwrap();
        let other_id: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id='other_test'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(cu_id, 1, "cu_id should be 1 since it's the first insert");

        let witness_json =
            serde_json::json!([{"content_unit": cu_id, "origin": "Static"}]).to_string();
        conn.execute(
            "INSERT INTO provenance (run_id, test_item_id, selected, confidence, witness_json)
             VALUES ('prev-sel', ?1, 0, 0.0, '[]')",
            rusqlite::params![test_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO provenance (run_id, test_item_id, selected, confidence, witness_json)
             VALUES ('prev-sel', ?1, 1, 1.0, ?2)",
            rusqlite::params![other_id, witness_json],
        )
        .unwrap();

        // Ingest a full run where the test failed
        let results = serde_json::json!({
            "run_id": "full-run-2",
            "full_run": true,
            "tests": [{"id": test_id, "outcome": "failed", "duration_ms": 10}]
        });
        let incidents = store
            .detect_missed_selections("full-run-2", &results)
            .unwrap();
        assert_eq!(incidents, 1, "should detect one missed-selection incident");

        // Verify incident was recorded
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM missed_selection_incidents WHERE run_id='full-run-2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "incident should be recorded");

        // Verify manual edge was created
        let edge_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependency_edges
                 WHERE test_item_id=?1 AND origin='manual'",
                rusqlite::params![test_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 1, "manual edge should be created");
    }

    #[test]
    fn test_record_missed_selection_direct() {
        // Directly test record_missed_selection to isolate FK issues
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Set up content unit
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', 'abc123')",
            [],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Test item
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_x')",
            [],
        ).unwrap();
        let test_id: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Call record_missed_selection directly
        store
            .record_missed_selection("run-x", test_id, cu_id)
            .unwrap();

        // Verify incident was recorded
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM missed_selection_incidents WHERE run_id='run-x'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "incident should be recorded");

        // Verify manual edge exists
        let edge_count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM dependency_edges
                 WHERE test_item_id=?1 AND origin='manual' AND content_unit_id=?2",
                rusqlite::params![test_id, cu_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 1, "manual edge should exist");
    }

    #[test]
    fn test_detect_missed_selections_passed_test_skipped() {
        // A skipped test that PASSED should NOT trigger an incident
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_b')",
            [],
        ).unwrap();
        let test_id: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Simulate prior selection: test was skipped
        conn.execute(
            "INSERT INTO provenance (run_id, test_item_id, selected, confidence, witness_json)
             VALUES ('prev-sel', ?1, 0, 0.0, '[]')",
            rusqlite::params![test_id],
        )
        .unwrap();

        // Full run: test PASSED
        let results = serde_json::json!({
            "run_id": "full-run-3",
            "full_run": true,
            "tests": [{"id": test_id, "outcome": "passed", "duration_ms": 10}]
        });
        let incidents = store
            .detect_missed_selections("full-run-3", &results)
            .unwrap();
        assert_eq!(incidents, 0, "passed test should not trigger incident");
    }

    #[test]
    fn test_detect_missed_selections_not_full_run() {
        // A non-full-run should NOT trigger detection during ingest
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_c')",
            [],
        ).unwrap();
        let test_id: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Prior selection: test was skipped
        conn.execute(
            "INSERT INTO provenance (run_id, test_item_id, selected, confidence, witness_json)
             VALUES ('prev-sel', ?1, 0, 0.0, '[]')",
            rusqlite::params![test_id],
        )
        .unwrap();

        // Ingest without full_run flag — should NOT trigger detection
        let results = serde_json::json!({
            "run_id": "sel-run-1",
            "tests": [{"id": test_id, "outcome": "failed", "duration_ms": 10}]
        });
        store.ingest(&results).unwrap();

        // Verify no incidents were created by the ingest
        let count: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM missed_selection_incidents WHERE run_id='sel-run-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "non-full-run should not create incidents");
    }

    #[test]
    fn test_missed_selection_manual_edge_in_selection() {
        // Verify that the manual edge forces selection in future runs
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Set up content unit
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', ?1)",
            rusqlite::params![fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Test item
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_d')",
            [],
        ).unwrap();
        let test_id: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Give test a run history so it's not always-run (no history)
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![test_id],
        )
        .unwrap();

        // Create a manual edge (simulating what record_missed_selection does)
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'manual', 1000000)",
            rusqlite::params![test_id, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, test_id],
        )
        .unwrap();

        // Change the file
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        let engine = crate::engine::Engine::new(&store);
        let delta = crate::change::ChangeSet {
            files: vec!["src/lib.rs".to_string()],
            base: None,
            head: None,
        };
        let sel = engine.select(&delta).unwrap();

        // Test should be selected via the manual edge
        let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
        assert!(
            ids.contains(&test_id),
            "test should be selected via manual edge"
        );

        // Verify the witness includes the manual origin
        let test = sel.tests.iter().find(|t| t.id == test_id).unwrap();
        let has_manual = test
            .witness
            .as_ref()
            .map(|w| w.iter().any(|e| e.origin == crate::engine::Origin::Manual))
            .unwrap_or(false);
        assert!(has_manual, "witness should include manual edge origin");
    }

    // ── Must-run rules tests (TIA-SAFE-009) ──

    #[test]
    fn test_must_run_rule_adds_test_to_always_run() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        // Create config with must-run rule
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"
[must_run]
"*.config" = ["config-test-node"]
"#,
        )
        .unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Insert content unit
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'app.config', NULL, 'source', 'abc123')",
            [],
        )
        .unwrap();

        // Insert test item with matching node_id
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'config-test-node')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Give test a run history so it's not always-run by default
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Selection with app.config changed should add the test via must-run
        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec!["app.config".to_string()],
                base: None,
                head: None,
            })
            .unwrap();

        assert!(
            ctx.always_run.contains(&tid),
            "must-run rule should force-select the test when matching file changes"
        );
    }

    #[test]
    fn test_must_run_non_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"
[must_run]
"*.secret" = ["secret-test"]
"#,
        )
        .unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'secret-test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Change a .rs file, not .secret — must-run should NOT trigger
        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec!["src/main.rs".to_string()],
                base: None,
                head: None,
            })
            .unwrap();

        assert!(
            !ctx.always_run.contains(&tid),
            "must-run should not trigger for non-matching file"
        );
    }

    // ── Periodic full-run tests (TIA-SAFE-006) ──

    #[test]
    fn test_periodic_full_run_selects_all_tests() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"
[periodic_full_run]
interval_hours = 0
"#,
        )
        .unwrap();
        // interval_hours = 0 means disabled — no automatic full run

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 't1')",
            [],
        )
        .unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // With interval=0, periodic full-run is disabled; test should NOT
        // be always-run (it has history and no other reason). Only selected
        // if change reaches it.
        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert!(
            !ctx.always_run.contains(&tid),
            "disabled periodic full run should not force-select tests"
        );
    }

    // ── Environment fingerprinting tests (TIA-CORE-008, TIA-RUN-006) ──

    #[test]
    fn test_resolve_environment_default() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let fp = store.resolve_environment(None, None).unwrap();
        assert_eq!(fp, "default", "no metadata should return 'default'");
    }

    #[test]
    fn test_resolve_environment_with_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let fp1 = store
            .resolve_environment(Some("rustc-1.80"), Some("linux"))
            .unwrap();
        assert_ne!(fp1, "default", "should return a non-default fingerprint");
        assert!(
            fp1.starts_with("env-"),
            "fingerprint should start with 'env-'"
        );

        // Same metadata should return the same fingerprint
        let fp2 = store
            .resolve_environment(Some("rustc-1.80"), Some("linux"))
            .unwrap();
        assert_eq!(fp1, fp2, "same metadata should return same fingerprint");
    }

    #[test]
    fn test_ingest_records_environment() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'env_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Ingest with environment metadata
        let payload = serde_json::json!({
            "run_id": "env-run-1",
            "environment": {"toolchain": "rustc-1.80", "os": "linux"},
            "tests": [{"id": tid, "outcome": "passed", "duration_ms": 10}]
        });
        store.ingest(&payload).unwrap();

        // Verify environment was recorded in run_history
        let env: String = conn
            .query_row(
                "SELECT environment FROM run_history WHERE run_id='env-run-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            env.starts_with("env-"),
            "environment should be a fingerprint, got: {}",
            env
        );
        assert_ne!(env, "default", "environment should not be 'default'");
    }

    #[test]
    fn test_environment_scoped_edge_query() {
        // Verify that edges from 'env-a' environment are properly scoped.
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join(".testaruda");

        // Write config with environment = env-a
        std::fs::write(
            dir.path().join("testaruda.toml"),
            r#"
[environment]
name = "env-a"
"#,
        )
        .unwrap();

        let store = Store::open(store_path).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Content unit — create file on disk
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();

        // Seed the CU with a placeholder fingerprint
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', 'seed-fp')",
            [],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Test item
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'env_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Insert edge in 'env-a' environment
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'env-a', 'static', 1000000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();

        // Give test a run history so it's not always-run
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Verify the config environment name is picked up
        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec![],
                base: None,
                head: None,
            })
            .unwrap();
        assert_eq!(
            ctx.current_environment, "env-a",
            "environment should be 'env-a' from config"
        );

        // Now change the file so fingerprints differ
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        let delta = crate::change::ChangeSet {
            files: vec!["src/lib.rs".to_string()],
            base: None,
            head: None,
        };
        let ctx = store.load_selection_context(&delta).unwrap();
        assert!(!ctx.changed.is_empty(), "no changed CUs");
        assert!(!ctx.test_deps.is_empty(), "env-a should have dep edges");
        assert_eq!(
            ctx.current_environment, "env-a",
            "env should be env-a from config"
        );

        let engine = crate::engine::Engine::new(&store);
        let sel = engine
            .select_with_context(ctx, crate::engine::TestOrdering::Default)
            .unwrap();

        // The test should be selected via the env-a edge
        let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
        assert!(
            ids.contains(&tid),
            "test should be selected via env-a edge. selected: {:?}",
            ids
        );
    }

    #[test]
    fn test_environment_isolated_edge_queries() {
        // Two stores with different environments — edges from one should
        // not leak into the other's selection context.
        //
        // Uses raw SQL for env-a edges and the store's load_selection_context
        // with default env.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Content unit
        let file_path = dir.path().join("src/lib.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn foo() {}").unwrap();
        let fp = Store::compute_fingerprint(&file_path).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', ?1)",
            rusqlite::params![fp],
        )
        .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();

        // Two test items
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 't1')",
            [],
        )
        .unwrap();
        let tid1: u32 = conn
            .query_row("SELECT id FROM test_items WHERE node_id='t1'", [], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 't2')",
            [],
        )
        .unwrap();
        let tid2: u32 = conn
            .query_row("SELECT id FROM test_items WHERE node_id='t2'", [], |row| {
                row.get(0)
            })
            .unwrap();

        // Edge for t1 in 'env-a' environment
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'env-a', 'static', 1000000)",
            rusqlite::params![tid1, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid1],
        )
        .unwrap();

        // Edge for t2 in 'default' environment
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid2, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid2],
        )
        .unwrap();

        // Give both tests run history so they're not always-run
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed', 'passed', 10, 'default')",
            rusqlite::params![tid2],
        )
        .unwrap();

        // Change the file
        std::fs::write(&file_path, b"fn bar() {}").unwrap();

        // Load selection context with default environment
        let ctx = store
            .load_selection_context(&crate::change::ChangeSet {
                files: vec!["src/lib.rs".to_string()],
                base: None,
                head: None,
            })
            .unwrap();

        // Only t2's edge (environment='default') should be in test_deps
        let dep_ids: std::collections::HashSet<u32> =
            ctx.test_deps.iter().map(|&(t, ..)| t).collect();
        assert!(
            !dep_ids.contains(&tid1),
            "env-a edge should not appear in default env query"
        );
        assert!(
            dep_ids.contains(&tid2),
            "default env edge should appear in default env query"
        );
    }

    // ── Graph export/import tests (TIA-STORE-003) ──

    #[test]
    fn test_graph_export_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Seed some data
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', 'abc123')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_a')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'r1', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        // Export
        let graph = store.export_graph().unwrap();

        // Verify format
        assert_eq!(graph["format"], "testaruda-graph-v1");
        assert!(graph["content_units"].as_array().unwrap().len() == 1);
        assert!(graph["test_items"].as_array().unwrap().len() == 1);
        assert!(graph["edges"].as_array().unwrap().len() == 1);
        assert!(graph["run_history"].as_array().unwrap().len() == 1);

        // Verify content unit details
        let cu = &graph["content_units"][0];
        assert_eq!(cu["path"], "src/lib.rs");
        assert_eq!(cu["kind"], "source");
        assert_eq!(cu["fingerprint"], "abc123");

        // Verify test item details
        let ti = &graph["test_items"][0];
        assert_eq!(ti["node_id"], "test_a");
        assert_eq!(ti["quarantined"], false);

        // Verify edge details
        let edge = &graph["edges"][0];
        assert_eq!(edge["origin"], "static");
        assert_eq!(edge["k"], 1000000);

        // Verify run history details
        let run = &graph["run_history"][0];
        assert_eq!(run["outcome"], "passed");
        assert_eq!(run["duration_ms"], 10);
    }

    #[test]
    fn test_graph_import_reconstructs_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        // Import a graph
        let graph = serde_json::json!({
            "format": "testaruda-graph-v1",
            "content_units": [
                {"id": 1, "component": "default", "path": "src/lib.rs", "symbol": null, "kind": "source", "fingerprint": "abc123"}
            ],
            "test_items": [
                {"id": 10, "component": "default", "adapter": "test", "node_id": "test_a", "quarantined": false}
            ],
            "edges": [
                {"from": 10, "to": 1, "from_node_id": "test_a", "to_path": "src/lib.rs", "environment": "default", "origin": "static", "k": 1000000}
            ],
            "run_history": [
                {"test_item_id": 10, "node_id": "test_a", "run_id": "r1", "outcome": "passed", "duration_ms": 10, "environment": "default"}
            ]
        });

        store.import_graph(&graph).unwrap();

        // Verify content unit was created
        let cu_count: u32 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM content_units", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cu_count, 1, "should import 1 content unit");

        // Verify test item was created
        let ti_count: u32 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM test_items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ti_count, 1, "should import 1 test item");

        // Verify edge was created
        let edge_count: u32 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM dependency_edges", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(edge_count, 1, "should import 1 edge");

        // Verify reverse index was created
        let ri_count: u32 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM reverse_index", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ri_count, 1, "should import 1 reverse index entry");

        // Verify run history was created
        let rh_count: u32 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM run_history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rh_count, 1, "should import 1 run history entry");
    }

    #[test]
    fn test_graph_import_unknown_format_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let graph = serde_json::json!({"format": "unknown-format", "nodes": []});
        let result = store.import_graph(&graph);
        assert!(result.is_err(), "unknown format should be rejected");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown graph format"),
            "error should mention unknown format"
        );
    }

    #[test]
    fn test_graph_export_import_roundtrip() {
        // Export from one store, import into another, verify identical state
        let dir = tempfile::tempdir().unwrap();

        let store1 = Store::open(dir.path().join("store1")).unwrap();
        store1.initialize().unwrap();
        let conn1 = store1.conn();

        conn1
            .execute(
                "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/app.rs', NULL, 'source', 'def456')",
                [],
            )
            .unwrap();
        let cu_id: u32 = conn1
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();
        conn1.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_b')",
            [],
        ).unwrap();
        let tid: u32 = conn1
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        conn1.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'runtime', 500000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn1
            .execute(
                "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
                rusqlite::params![cu_id, tid],
            )
            .unwrap();

        // Export
        let graph = store1.export_graph().unwrap();

        // Import into a second store
        let store2 = Store::open(dir.path().join("store2")).unwrap();
        store2.initialize().unwrap();
        store2.import_graph(&graph).unwrap();

        // Verify both stores have the same data
        let cu_count2: u32 = store2
            .conn()
            .query_row("SELECT COUNT(*) FROM content_units", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cu_count2, 1, "store2 should have 1 content unit");

        let edge_count2: u32 = store2
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM dependency_edges WHERE origin='runtime'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(edge_count2, 1, "store2 should have 1 runtime edge");
    }

    // ── Soufflé oracle tests (TIA-ENG-010) ──

    #[test]
    fn test_generate_datalog_basic() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Seed some data
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', 'src/lib.rs', NULL, 'source', 'abc123')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'test_a')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();
        let cu_id: u32 = conn
            .query_row("SELECT id FROM content_units", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', 1000000)",
            rusqlite::params![tid, cu_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO reverse_index (content_unit_id, test_item_id) VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();

        let datalog = store.generate_datalog().unwrap();

        // Should contain the relation declarations
        assert!(datalog.contains(".decl changed"));
        assert!(datalog.contains(".decl affected"));
        assert!(datalog.contains(".decl output_affected"));

        // Should contain the rules
        assert!(datalog.contains("impacted(cu) :- changed(cu)"));
        assert!(datalog.contains("output_affected(t) :- affected(t, _)"));

        // Should contain the test_dep facts
        assert!(datalog.contains(&format!(
            "test_dep({}, {}, \"static\", 1000000)",
            tid, cu_id
        )));

        // Should contain the output directive
        assert!(datalog.contains(".output output_affected"));
    }

    #[test]
    fn test_generate_datalog_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let datalog = store.generate_datalog().unwrap();

        // Should still produce valid Datalog with declarations
        assert!(datalog.contains(".decl changed"));
        assert!(datalog.contains(".output output_affected"));
        // No facts (lines ending with ")." without ":-" — rules have ":-")
        // In an empty store, there should be no facts
        let fact_lines: Vec<&str> = datalog
            .lines()
            .filter(|l| l.ends_with(").") && !l.contains(":-"))
            .collect();
        assert_eq!(
            fact_lines.len(),
            0,
            "empty store should have no facts, got: {:?}",
            fact_lines
        );
    }

    // ── Predictive ranking calibration gate (TIA-VER-005) ──

    #[test]
    fn test_ranking_calibration_recall_computed() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();

        // Insert 3 test items
        let mut tids = Vec::new();
        for name in ["test_a", "test_b", "test_c"] {
            conn.execute(
                "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', ?1)",
                rusqlite::params![name],
            ).unwrap();
            let tid: u32 = conn
                .query_row(
                    "SELECT id FROM test_items WHERE node_id = ?1",
                    rusqlite::params![name],
                    |row| row.get(0),
                )
                .unwrap();
            tids.push(tid);
        }

        // Training runs: test_a failed 3/3, test_b failed 0/3, test_c failed 2/3
        // So predicted order: test_a (100%) > test_c (67%) > test_b (0%)
        for i in 0..3 {
            for &tid in &tids {
                let is_failed = tid == tids[0] || (tid == tids[2] && i < 2);
                let outcome = if is_failed { "failed" } else { "passed" };
                let run_id = format!("train-{}", i);
                conn.execute(
                    "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                     VALUES (?1, ?2, ?3, 10, 'default')",
                    rusqlite::params![tid, run_id, outcome],
                ).unwrap();
            }
        }

        // Test runs (hold-out): test_a fails again, test_b passes, test_c fails
        // top-k should capture test_a and test_c (the failures)
        for &tid in &tids {
            let outcome = if tid == tids[1] { "passed" } else { "failed" };
            conn.execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, 'test-run', ?2, 10, 'default')",
                rusqlite::params![tid, outcome],
            )
            .unwrap();
        }

        let metrics = store.evaluate_ranking_calibration().unwrap();

        assert_eq!(metrics.total_test_items, 3);
        assert_eq!(metrics.total_failures, 2);
        assert_eq!(metrics.captured_failures, 2);
        assert_eq!(metrics.recall_at_k, 1.0);
        assert_eq!(metrics.k, 3);
    }

    #[test]
    fn test_ranking_calibration_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let metrics = store.evaluate_ranking_calibration().unwrap();
        assert_eq!(metrics.total_test_items, 0);
        assert_eq!(metrics.total_failures, 0);
        assert_eq!(metrics.recall_at_k, 0.0);
    }

    #[test]
    fn test_ranking_calibration_no_hold_out() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let conn = store.conn();
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'only_test')",
            [],
        ).unwrap();
        let tid: u32 = conn
            .query_row("SELECT id FROM test_items", [], |row| row.get(0))
            .unwrap();

        // Only one run — no separate hold-out possible
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'only-run', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();

        let metrics = store.evaluate_ranking_calibration().unwrap();
        assert_eq!(metrics.total_test_items, 0, "no hold-out set available");
        assert_eq!(metrics.total_failures, 0);
        assert_eq!(metrics.recall_at_k, 0.0);
    }
}
