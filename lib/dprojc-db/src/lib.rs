use dprojc_types::{Project, ProjectIndicator, ProjectType, ScanError, ScanResult, ScanResultSummary, ScanStatistics};
use dprojc_utils::default_db_path;
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use std::path::Path;
use thiserror::Error;

/// Database errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("Path conversion error: {0}")]
    Path(String),
    #[error("Database migration error: {0}")]
    Migration(String),
    #[error("Project not found: {0}")]
    ProjectNotFound(String),
}

/// Result type for database operations
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// Database schema version
const CURRENT_SCHEMA_VERSION: i32 = 2;

/// Database schema definitions
mod schema {
    pub const CREATE_PROJECTS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            project_type TEXT NOT NULL,
            last_scanned TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    pub const CREATE_INDICATORS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS project_indicators (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            indicator_type TEXT NOT NULL,
            indicator_value TEXT,
            FOREIGN KEY (project_id) REFERENCES projects (id) ON DELETE CASCADE
        )
    "#;

    pub const CREATE_SCAN_RESULTS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS scan_results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scan_timestamp TEXT NOT NULL,
            root_path TEXT NOT NULL,
            dirs_scanned INTEGER NOT NULL,
            scan_duration_ms INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    pub const CREATE_SCAN_ERRORS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS scan_errors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scan_result_id INTEGER NOT NULL,
            error_path TEXT NOT NULL,
            error_type TEXT NOT NULL,
            error_message TEXT NOT NULL,
            FOREIGN KEY (scan_result_id) REFERENCES scan_results (id) ON DELETE CASCADE
        )
    "#;

    pub const CREATE_EXCLUDED_DIRS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS excluded_dirs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scan_result_id INTEGER NOT NULL,
            dir_path TEXT NOT NULL,
            FOREIGN KEY (scan_result_id) REFERENCES scan_results (id) ON DELETE CASCADE
        )
    "#;

    pub const CREATE_SCAN_PROJECTS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS scan_projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scan_result_id INTEGER NOT NULL,
            project_id INTEGER NOT NULL,
            FOREIGN KEY (scan_result_id) REFERENCES scan_results (id) ON DELETE CASCADE,
            FOREIGN KEY (project_id) REFERENCES projects (id) ON DELETE CASCADE
        )
    "#;

    pub const CREATE_SCHEMA_VERSION_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        )
    "#;

    pub const CREATE_MIGRATIONS_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            migration_name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
    "#;

    // Indexes for performance
    pub const CREATE_PROJECTS_PATH_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_projects_path ON projects (path)
    "#;

    pub const CREATE_PROJECTS_TYPE_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_projects_type ON projects (project_type)
    "#;

    pub const CREATE_INDICATORS_PROJECT_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_indicators_project ON project_indicators (project_id)
    "#;

    pub const CREATE_SCAN_RESULTS_TIMESTAMP_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_scan_results_timestamp ON scan_results (scan_timestamp)
    "#;

    // Additional performance indexes


    pub const CREATE_SCAN_ERRORS_SCAN_RESULT_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_scan_errors_scan_result ON scan_errors (scan_result_id)
    "#;

    pub const CREATE_EXCLUDED_DIRS_SCAN_RESULT_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_excluded_dirs_scan_result ON excluded_dirs (scan_result_id)
    "#;

    pub const CREATE_SCAN_PROJECTS_SCAN_RESULT_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_scan_projects_scan_result ON scan_projects (scan_result_id)
    "#;

    pub const CREATE_SCAN_PROJECTS_PROJECT_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS idx_scan_projects_project ON scan_projects (project_id)
    "#;
}

/// Main database struct
pub struct ProjectDatabase {
    conn: Connection,
}

impl ProjectDatabase {
    /// Open a database connection at the default path
    pub fn open_default() -> Result<Self> {
        let db_path = default_db_path()?;
        Self::open(&db_path)
    }

    /// Open a database connection at a specific path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let mut db = Self { conn };

        // Enable foreign keys
        db.conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Initialize schema
        db.initialize_schema()?;

        Ok(db)
    }

    /// Initialize the database schema
    fn initialize_schema(&mut self) -> Result<()> {
        // Create all tables
        self.conn.execute(schema::CREATE_SCHEMA_VERSION_TABLE, [])?;
        self.conn.execute(schema::CREATE_MIGRATIONS_TABLE, [])?;
        self.conn.execute(schema::CREATE_PROJECTS_TABLE, [])?;
        self.conn.execute(schema::CREATE_INDICATORS_TABLE, [])?;
        self.conn.execute(schema::CREATE_SCAN_RESULTS_TABLE, [])?;
        self.conn.execute(schema::CREATE_SCAN_ERRORS_TABLE, [])?;
        self.conn.execute(schema::CREATE_EXCLUDED_DIRS_TABLE, [])?;
        self.conn.execute(schema::CREATE_SCAN_PROJECTS_TABLE, [])?;

        // Create indexes
        self.conn.execute(schema::CREATE_PROJECTS_PATH_INDEX, [])?;
        self.conn.execute(schema::CREATE_PROJECTS_TYPE_INDEX, [])?;

        self.conn.execute(schema::CREATE_INDICATORS_PROJECT_INDEX, [])?;
        self.conn.execute(schema::CREATE_SCAN_RESULTS_TIMESTAMP_INDEX, [])?;
        self.conn.execute(schema::CREATE_SCAN_ERRORS_SCAN_RESULT_INDEX, [])?;
        self.conn.execute(schema::CREATE_EXCLUDED_DIRS_SCAN_RESULT_INDEX, [])?;
        self.conn.execute(schema::CREATE_SCAN_PROJECTS_SCAN_RESULT_INDEX, [])?;
        self.conn.execute(schema::CREATE_SCAN_PROJECTS_PROJECT_INDEX, [])?;

        // Run migrations
        self.run_migrations()?;

        // Set schema version
        self.set_schema_version(CURRENT_SCHEMA_VERSION)?;

        Ok(())
    }

    /// Run database migrations
    fn run_migrations(&self) -> Result<()> {
        // Get current schema version
        let current_version = self.get_schema_version().unwrap_or(0);

        // Run migrations from current version to latest
        if current_version < 1 {
            self.run_migration("initial_schema", || {
                // Migration logic is already handled in initialize_schema
                Ok(())
            })?;
        }

        if current_version < 2 {
            self.run_migration("add_frecency_tracking", || {
                // Add frecency tracking columns to projects table
                self.conn.execute(
                    "ALTER TABLE projects ADD COLUMN frecency_score REAL NOT NULL DEFAULT 0.0",
                    [],
                )?;
                self.conn.execute(
                    "ALTER TABLE projects ADD COLUMN last_accessed INTEGER",
                    [],
                )?;
                self.conn.execute(
                    "ALTER TABLE projects ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;

                // Create index on frecency score for faster lookups
                self.conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_projects_frecency ON projects(frecency_score DESC)",
                    [],
                )?;

                Ok(())
            })?;
        }

        Ok(())
    }

    /// Run a single migration
    fn run_migration<F>(&self, migration_name: &str, migration_fn: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        // Check if migration has already been applied
        let already_applied: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM migrations WHERE migration_name = ?",
            params![migration_name],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        )?;

        if already_applied {
            return Ok(());
        }

        // Run the migration
        migration_fn()?;

        // Record the migration as applied
        self.conn.execute(
            "INSERT INTO migrations (migration_name) VALUES (?)",
            params![migration_name],
        )?;

        Ok(())
    }

    /// Get the current schema version
    pub fn get_schema_version(&self) -> Result<i32> {
        let version: i32 = self.conn.query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Set the schema version
    fn set_schema_version(&self, version: i32) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?)",
            params![version],
        )?;
        Ok(())
    }

    /// Begin a database transaction
    pub fn begin_transaction(&mut self) -> Result<Transaction<'_>> {
        Ok(self.conn.transaction()?)
    }

    /// Insert or update a project
    pub fn upsert_project(&mut self, project: &Project) -> Result<i64> {
        let tx = self.conn.transaction()?;

        // Insert or replace project
        tx.execute(
            r#"
            INSERT OR REPLACE INTO projects (path, project_type, last_scanned, updated_at)
            VALUES (?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            params![
                project.path.to_string_lossy(),
                serde_json::to_string(&project.project_type)?,
                project.last_scanned.to_rfc3339()
            ],
        )?;

        let project_id = tx.last_insert_rowid();

        // Delete existing indicators for this project
        tx.execute(
            "DELETE FROM project_indicators WHERE project_id = ?",
            params![project_id],
        )?;

        // Insert new indicators
        for indicator in &project.indicators {
            let indicator_value = match indicator {
                ProjectIndicator::Custom(s) => Some(s.clone()),
                _ => None,
            };
            tx.execute(
                "INSERT INTO project_indicators (project_id, indicator_type, indicator_value) VALUES (?, ?, ?)",
                params![
                    project_id,
                    serde_json::to_string(indicator)?,
                    indicator_value
                ],
            )?;
        }

        tx.commit()?;
        Ok(project_id)
    }

    /// Get a project by path
    pub fn get_project_by_path<P: AsRef<Path>>(&self, path: P) -> Result<Option<Project>> {
        let path_str = path.as_ref().to_string_lossy();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned
            FROM projects p
            WHERE p.path = ?
            "#,
        )?;

        let mut rows = stmt.query_map(params![path_str], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
            ))
        })?;

        if let Some(row_result) = rows.next() {
            let (id, path_str, project_type, last_scanned) = row_result?;

            // Get indicators
            let mut indicators = Vec::new();
            let mut indicator_stmt = self.conn.prepare(
                "SELECT indicator_type FROM project_indicators WHERE project_id = ?",
            )?;
            let indicator_rows = indicator_stmt.query_map(params![id], |row| {
                let indicator_json: String = row.get(0)?;
                serde_json::from_str(&indicator_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
            })?;

            for indicator_result in indicator_rows {
                indicators.push(indicator_result?);
            }

            let project = Project {
                path: std::path::PathBuf::from(path_str),
                project_type,
                indicators,
                last_scanned,
            };

            Ok(Some(project))
        } else {
            Ok(None)
        }
    }

    /// Get all projects
    pub fn get_all_projects(&self) -> Result<Vec<Project>> {
        // Use a JOIN to fetch indicators in a single query (avoid N+1)
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned, pi.indicator_type
            FROM projects p
            LEFT JOIN project_indicators pi ON p.id = pi.project_id
            ORDER BY p.path, pi.indicator_type
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;
            let indicator_json: Option<String> = row.get(4)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
                indicator_json,
            ))
        })?;

        // Group results by project (since JOIN creates one row per indicator)
        let mut projects_map: std::collections::HashMap<String, (i64, ProjectType, chrono::DateTime<chrono::Utc>, Vec<ProjectIndicator>)> = std::collections::HashMap::new();

        for row_result in rows {
            let (id, path_str, project_type, last_scanned, indicator_json) = row_result?;

            let entry = projects_map.entry(path_str.clone()).or_insert((id, project_type, last_scanned, Vec::new()));

            if let Some(ind_json) = indicator_json {
                let indicator: ProjectIndicator = serde_json::from_str(&ind_json)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                entry.3.push(indicator);
            }
        }

        // Convert to Vec and sort by path
        let mut projects: Vec<Project> = projects_map
            .into_iter()
            .map(|(path_str, (_, project_type, last_scanned, indicators))| Project {
                path: std::path::PathBuf::from(path_str),
                project_type,
                indicators,
                last_scanned,
            })
            .collect();

        projects.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(projects)
    }

    /// Delete a project by path
    pub fn delete_project_by_path<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let path_str = path.as_ref().to_string_lossy();
        let rows_affected = self.conn.execute(
            "DELETE FROM projects WHERE path = ?",
            params![path_str],
        )?;
        Ok(rows_affected > 0)
    }

    /// Get projects by type
    pub fn get_projects_by_type(&self, project_type: &ProjectType) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned
            FROM projects p
            WHERE p.project_type = ?
            ORDER BY p.path
            "#,
        )?;

        let project_iter = stmt.query_map(params![serde_json::to_string(project_type)?], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
            ))
        })?;

        let mut projects = Vec::new();
        for row_result in project_iter {
            let (id, path_str, project_type, last_scanned) = row_result?;

            // Get indicators
            let mut indicators = Vec::new();
            let mut indicator_stmt = self.conn.prepare(
                "SELECT indicator_type FROM project_indicators WHERE project_id = ?",
            )?;
            let indicator_rows = indicator_stmt.query_map(params![id], |row| {
                let indicator_json: String = row.get(0)?;
                serde_json::from_str(&indicator_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
            })?;

            for indicator_result in indicator_rows {
                indicators.push(indicator_result?);
            }

            let project = Project {
                path: std::path::PathBuf::from(path_str),
                project_type,
                indicators,
                last_scanned,
            };

            projects.push(project);
        }

        Ok(projects)
    }

    /// Get projects by indicator type
    pub fn get_projects_by_indicator(&self, indicator: &ProjectIndicator) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned
            FROM projects p
            INNER JOIN project_indicators pi ON p.id = pi.project_id
            WHERE pi.indicator_type = ?
            ORDER BY p.path
            "#,
        )?;

        let project_iter = stmt.query_map(params![serde_json::to_string(indicator)?], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
            ))
        })?;

        let mut projects = Vec::new();
        for row_result in project_iter {
            let (id, path_str, project_type, last_scanned) = row_result?;

            // Get indicators
            let mut indicators = Vec::new();
            let mut indicator_stmt = self.conn.prepare(
                "SELECT indicator_type FROM project_indicators WHERE project_id = ?",
            )?;
            let indicator_rows = indicator_stmt.query_map(params![id], |row| {
                let indicator_json: String = row.get(0)?;
                serde_json::from_str(&indicator_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
            })?;

            for indicator_result in indicator_rows {
                indicators.push(indicator_result?);
            }

            let project = Project {
                path: std::path::PathBuf::from(path_str),
                project_type,
                indicators,
                last_scanned,
            };

            projects.push(project);
        }

        Ok(projects)
    }

    /// Get project counts by type
    pub fn get_project_counts_by_type(&self) -> Result<std::collections::HashMap<ProjectType, usize>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT project_type, COUNT(*)
            FROM projects
            GROUP BY project_type
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let project_type_json: String = row.get(0)?;
            let count: i64 = row.get(1)?;

            Ok((
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                count as usize,
            ))
        })?;

        let mut counts = std::collections::HashMap::new();
        for row_result in rows {
            let (project_type, count) = row_result?;
            counts.insert(project_type, count);
        }

        Ok(counts)
    }

    /// Search projects by path pattern
    pub fn search_projects_by_path(&self, pattern: &str) -> Result<Vec<Project>> {
        self.search_projects_by_path_limit(pattern, None)
    }

    /// Search projects by path pattern with optional limit
    pub fn search_projects_by_path_limit(&self, pattern: &str, limit: Option<usize>) -> Result<Vec<Project>> {
        let like_pattern = format!("%{}%", pattern);

        let query = if let Some(lim) = limit {
            format!(
                r#"
                SELECT p.id, p.path, p.project_type, p.last_scanned, pi.indicator_type
                FROM projects p
                LEFT JOIN project_indicators pi ON p.id = pi.project_id
                WHERE p.path LIKE ?
                ORDER BY LENGTH(p.path), p.path, pi.indicator_type
                LIMIT {}
                "#,
                lim
            )
        } else {
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned, pi.indicator_type
            FROM projects p
            LEFT JOIN project_indicators pi ON p.id = pi.project_id
            WHERE p.path LIKE ?
            ORDER BY LENGTH(p.path), p.path, pi.indicator_type
            "#.to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map(params![like_pattern], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;
            let indicator_json: Option<String> = row.get(4)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
                indicator_json,
            ))
        })?;

        // Group results by project
        let mut projects_map: std::collections::HashMap<String, (i64, ProjectType, chrono::DateTime<chrono::Utc>, Vec<ProjectIndicator>)> = std::collections::HashMap::new();
        let mut project_order: Vec<String> = Vec::new();

        for row_result in rows {
            let (id, path_str, project_type, last_scanned, indicator_json) = row_result?;

            if !projects_map.contains_key(&path_str) {
                project_order.push(path_str.clone());
                projects_map.insert(path_str.clone(), (id, project_type, last_scanned, Vec::new()));
            }

            if let Some(ind_json) = indicator_json {
                let indicator: ProjectIndicator = serde_json::from_str(&ind_json)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                if let Some(entry) = projects_map.get_mut(&path_str) {
                    entry.3.push(indicator);
                }
            }

            // Respect the limit on number of projects
            if let Some(lim) = limit {
                if projects_map.len() >= lim {
                    break;
                }
            }
        }

        // Maintain order (shortest paths first, then alphabetical)
        let projects: Vec<Project> = project_order
            .into_iter()
            .filter_map(|path_str| {
                projects_map.remove(&path_str).map(|(_, project_type, last_scanned, indicators)| Project {
                    path: std::path::PathBuf::from(path_str),
                    project_type,
                    indicators,
                    last_scanned,
                })
            })
            .collect();

        Ok(projects)
    }

    /// Store a scan result
    pub fn store_scan_result(&mut self, scan_result: &ScanResult) -> Result<i64> {
        let tx = self.conn.transaction()?;

        // Insert scan result
        tx.execute(
            r#"
            INSERT INTO scan_results (scan_timestamp, root_path, dirs_scanned, scan_duration_ms)
            VALUES (?, ?, ?, ?)
            "#,
            params![
                chrono::Utc::now().to_rfc3339(),
                scan_result.root_path.to_string_lossy(),
                scan_result.dirs_scanned as i64,
                scan_result.scan_duration_ms as i64
            ],
        )?;

        let scan_result_id = tx.last_insert_rowid();

        // Store errors
        for error in &scan_result.errors {
            tx.execute(
                "INSERT INTO scan_errors (scan_result_id, error_path, error_type, error_message) VALUES (?, ?, ?, ?)",
                params![
                    scan_result_id,
                    error.path.to_string_lossy(),
                    serde_json::to_string(&error.error_type)?,
                    error.message
                ],
            )?;
        }

        // Store excluded directories
        for excluded_dir in &scan_result.excluded_dirs {
            tx.execute(
                "INSERT INTO excluded_dirs (scan_result_id, dir_path) VALUES (?, ?)",
                params![
                    scan_result_id,
                    excluded_dir.to_string_lossy()
                ],
            )?;
        }

        // Store projects (this will also update existing ones)
        for project in &scan_result.projects {
            let project_id = ProjectDatabase::upsert_project_with_tx(&tx, project)?;

            // Link project to this scan result
            tx.execute(
                "INSERT INTO scan_projects (scan_result_id, project_id) VALUES (?, ?)",
                params![scan_result_id, project_id],
            )?;
        }

        tx.commit()?;
        Ok(scan_result_id)
    }

    /// Helper method to upsert project within a transaction
    fn upsert_project_with_tx(tx: &Transaction, project: &Project) -> Result<i64> {
        // Insert or replace project
        tx.execute(
            r#"
            INSERT OR REPLACE INTO projects (path, project_type, last_scanned, updated_at)
            VALUES (?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            params![
                project.path.to_string_lossy(),
                serde_json::to_string(&project.project_type)?,
                project.last_scanned.to_rfc3339()
            ],
        )?;

        let project_id = tx.last_insert_rowid();

        // Delete existing indicators for this project
        tx.execute(
            "DELETE FROM project_indicators WHERE project_id = ?",
            params![project_id],
        )?;

        // Insert new indicators
        for indicator in &project.indicators {
            let indicator_value = match indicator {
                ProjectIndicator::Custom(s) => Some(s.clone()),
                _ => None,
            };
            tx.execute(
                "INSERT INTO project_indicators (project_id, indicator_type, indicator_value) VALUES (?, ?, ?)",
                params![
                    project_id,
                    serde_json::to_string(indicator)?,
                    indicator_value
                ],
            )?;
        }

        Ok(project_id)
    }

    /// Get recent scan results
    pub fn get_recent_scan_results(&self, limit: usize) -> Result<Vec<ScanResultSummary>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, scan_timestamp, root_path, dirs_scanned, scan_duration_ms
            FROM scan_results
            ORDER BY scan_timestamp DESC
            LIMIT ?
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ScanResultSummary {
                id: row.get(0)?,
                scan_timestamp: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?
                    .with_timezone(&chrono::Utc),
                root_path: std::path::PathBuf::from(row.get::<_, String>(2)?),
                dirs_scanned: row.get::<_, i64>(3)? as usize,
                scan_duration_ms: row.get::<_, i64>(4)? as u64,
                error_count: 0, // Will be filled below
                excluded_dirs_count: 0, // Will be filled below
            })
        })?;

        let mut summaries = Vec::new();
        for row_result in rows {
            let mut summary = row_result?;

            // Get error count
            let error_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM scan_errors WHERE scan_result_id = ?",
                params![summary.id],
                |row| row.get(0),
            )?;
            summary.error_count = error_count as usize;

            // Get excluded dirs count
            let excluded_count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM excluded_dirs WHERE scan_result_id = ?",
                params![summary.id],
                |row| row.get(0),
            )?;
            summary.excluded_dirs_count = excluded_count as usize;

            summaries.push(summary);
        }

        Ok(summaries)
    }

    /// Get detailed scan result by ID
    pub fn get_scan_result(&self, scan_result_id: i64) -> Result<Option<ScanResult>> {
        // Get scan result metadata
        let (_scan_timestamp, root_path, dirs_scanned, scan_duration_ms): (String, String, i64, i64) = match self.conn.query_row(
            "SELECT scan_timestamp, root_path, dirs_scanned, scan_duration_ms FROM scan_results WHERE id = ?",
            params![scan_result_id],
            |row| Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
            )),
        ) {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };



        // Get errors
        let mut errors = Vec::new();
        let mut error_stmt = self.conn.prepare(
            "SELECT error_path, error_type, error_message FROM scan_errors WHERE scan_result_id = ?",
        )?;
        let error_rows = error_stmt.query_map(params![scan_result_id], |row| {
            let error_path: String = row.get(0)?;
            let error_type_json: String = row.get(1)?;
            let error_message: String = row.get(2)?;

            Ok(ScanError {
                path: std::path::PathBuf::from(error_path),
                error_type: serde_json::from_str(&error_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?,
                message: error_message,
            })
        })?;

        for error_result in error_rows {
            errors.push(error_result?);
        }

        // Get excluded directories
        let mut excluded_dirs = Vec::new();
        let mut excluded_stmt = self.conn.prepare(
            "SELECT dir_path FROM excluded_dirs WHERE scan_result_id = ?",
        )?;
        let excluded_rows = excluded_stmt.query_map(params![scan_result_id], |row| {
            let dir_path: String = row.get(0)?;
            Ok(std::path::PathBuf::from(dir_path))
        })?;

        for excluded_result in excluded_rows {
            excluded_dirs.push(excluded_result?);
        }

        // Get projects for this scan
        let mut projects = Vec::new();
        let mut project_stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path, p.project_type, p.last_scanned
            FROM projects p
            INNER JOIN scan_projects sp ON p.id = sp.project_id
            WHERE sp.scan_result_id = ?
            "#,
        )?;
        let project_rows = project_stmt.query_map(params![scan_result_id], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
            ))
        })?;

        for row_result in project_rows {
            let (id, path_str, project_type, last_scanned) = row_result?;

            // Get indicators for this project
            let mut indicators = Vec::new();
            let mut indicator_stmt = self.conn.prepare(
                "SELECT indicator_type FROM project_indicators WHERE project_id = ?",
            )?;
            let indicator_rows = indicator_stmt.query_map(params![id], |row| {
                let indicator_json: String = row.get(0)?;
                serde_json::from_str(&indicator_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
            })?;

            for indicator_result in indicator_rows {
                indicators.push(indicator_result?);
            }

            let project = Project {
                path: std::path::PathBuf::from(path_str),
                project_type,
                indicators,
                last_scanned,
            };

            projects.push(project);
        }

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from(root_path),
            projects,
            excluded_dirs,
            errors,
            dirs_scanned: dirs_scanned as usize,
            scan_duration_ms: scan_duration_ms as u64,
        };

        Ok(Some(scan_result))
    }

    /// Get scan statistics
    pub fn get_scan_statistics(&self) -> Result<ScanStatistics> {
        let total_scans: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM scan_results",
            [],
            |row| row.get(0),
        )?;

        let total_projects: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM projects",
            [],
            |row| row.get(0),
        )?;

        let total_dirs_scanned: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(dirs_scanned), 0) FROM scan_results",
            [],
            |row| row.get(0),
        )?;

        let total_errors: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM scan_errors",
            [],
            |row| row.get(0),
        )?;

        let last_scan_timestamp: Option<String> = self.conn.query_row(
            "SELECT scan_timestamp FROM scan_results ORDER BY scan_timestamp DESC LIMIT 1",
            [],
            |row| row.get(0),
        ).ok();

        let last_scan = if let Some(ts_str) = last_scan_timestamp {
            Some(chrono::DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| DatabaseError::Path(format!("Invalid timestamp: {}", e)))?
                .with_timezone(&chrono::Utc))
        } else {
            None
        };

        Ok(ScanStatistics {
            total_scans: total_scans as usize,
            total_projects: total_projects as usize,
            total_dirs_scanned: total_dirs_scanned as usize,
            total_errors: total_errors as usize,
            last_scan_timestamp: last_scan,
        })
    }

    /// Delete a scan result by ID
    pub fn delete_scan_result(&self, scan_result_id: i64) -> Result<bool> {
        let rows_affected = self.conn.execute(
            "DELETE FROM scan_results WHERE id = ?",
            params![scan_result_id],
        )?;
        Ok(rows_affected > 0)
    }

    /// Delete scan results older than the specified timestamp
    pub fn delete_old_scan_results(&self, before_timestamp: chrono::DateTime<chrono::Utc>) -> Result<usize> {
        let rows_affected = self.conn.execute(
            "DELETE FROM scan_results WHERE scan_timestamp < ?",
            params![before_timestamp.to_rfc3339()],
        )?;
        Ok(rows_affected)
    }

    /// Cleanup orphaned projects (projects not found in any recent scans)
    pub fn cleanup_orphaned_projects(&self, max_age_days: i64) -> Result<usize> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(max_age_days);

        // Find projects that haven't been scanned recently and aren't linked to recent scans
        let rows_affected = self.conn.execute(
            r#"
            DELETE FROM projects
            WHERE id NOT IN (
                SELECT DISTINCT sp.project_id
                FROM scan_projects sp
                INNER JOIN scan_results sr ON sp.scan_result_id = sr.id
                WHERE sr.scan_timestamp >= ?
            )
            "#,
            params![cutoff_date.to_rfc3339()],
        )?;

        Ok(rows_affected)
    }

    /// Backup database to SQL file
    pub fn backup_to_sql<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        use std::io::Write;

        let mut file = std::fs::File::create(path)?;

        // Write schema
        writeln!(file, "-- Database schema backup")?;
        writeln!(file, "-- Generated at: {}", chrono::Utc::now().to_rfc3339())?;
        writeln!(file)?;

        // Get all table schemas
        let tables = vec![
            "schema_version",
            "migrations",
            "projects",
            "project_indicators",
            "scan_results",
            "scan_errors",
            "excluded_dirs",
            "scan_projects",
        ];

        for table in &tables {
            // Get CREATE TABLE statement
            let create_stmt: String = self.conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
                params![table],
                |row| row.get(0),
            )?;

            writeln!(file, "{};", create_stmt)?;
            writeln!(file)?;
        }

        // Export data for each table
        for table in &tables {
            writeln!(file, "-- Data for table: {}", table)?;

            let sql = format!("SELECT * FROM {}", table);
            let mut stmt = self.conn.prepare(&sql)?;
            let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

            let rows = stmt.query_map([], |row| {
                let mut values = Vec::new();
                for i in 0..column_names.len() {
                    let value: rusqlite::types::Value = row.get(i)?;
                    values.push(value);
                }
                Ok(values)
            })?;

            for row_result in rows {
                let values = row_result?;
                let value_strs: Vec<String> = values.iter().map(|v| match v {
                    rusqlite::types::Value::Null => "NULL".to_string(),
                    rusqlite::types::Value::Integer(i) => i.to_string(),
                    rusqlite::types::Value::Real(r) => r.to_string(),
                    rusqlite::types::Value::Text(t) => format!("'{}'", t.replace('\'', "''")),
                    rusqlite::types::Value::Blob(b) => format!("X'{}'", hex::encode(b)),
                }).collect();

                writeln!(file, "INSERT INTO {} VALUES ({});", table, value_strs.join(", "))?;
            }
            writeln!(file)?;
        }

        Ok(())
    }

    /// Restore database from SQL file
    pub fn restore_from_sql<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // Drop all existing tables to allow clean restore
        self.drop_all_tables()?;

        let sql = std::fs::read_to_string(path)?;
        self.conn.execute_batch(&sql)?;
        Ok(())
    }

    /// Drop all tables (for restore operations)
    fn drop_all_tables(&self) -> Result<()> {
        let tables = vec![
            "scan_projects",
            "excluded_dirs",
            "scan_errors",
            "scan_results",
            "project_indicators",
            "projects",
            "migrations",
            "schema_version",
        ];

        for table in &tables {
            self.conn.execute(&format!("DROP TABLE IF EXISTS {}", table), [])?;
        }

        Ok(())
    }

    /// Check if a path has been scanned recently
    pub fn has_path_been_scanned_recently<P: AsRef<Path>>(&self, path: P, cutoff_time: chrono::DateTime<chrono::Utc>) -> Result<bool> {
        let path_str = path.as_ref().to_string_lossy();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM scan_results WHERE root_path = ? AND scan_timestamp > ?",
            params![path_str, cutoff_time.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Clear all data (for testing)
    pub fn clear_all_data(&self) -> Result<()> {
        self.conn.execute("DELETE FROM scan_errors", [])?;
        self.conn.execute("DELETE FROM excluded_dirs", [])?;
        self.conn.execute("DELETE FROM scan_projects", [])?;
        self.conn.execute("DELETE FROM project_indicators", [])?;
        self.conn.execute("DELETE FROM projects", [])?;
        self.conn.execute("DELETE FROM scan_results", [])?;
        self.conn.execute("DELETE FROM migrations", [])?;
        Ok(())
    }

    /// Record a directory access for frecency tracking
    /// Only records if the path is a project root in the database
    pub fn record_access<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let now = chrono::Utc::now().timestamp();

        // Check if this is a project root
        let project_id: Option<i64> = self.conn.query_row(
            "SELECT id, frecency_score, last_accessed, access_count FROM projects WHERE path = ?1",
            params![path_str],
            |row| {
                let id: i64 = row.get(0)?;
                let current_score: f64 = row.get(1)?;
                let last_accessed: Option<i64> = row.get(2)?;
                let access_count: i64 = row.get(3)?;

                // Calculate new score with time decay
                let new_score = if let Some(last_access) = last_accessed {
                    let time_diff = (now - last_access) as f64;
                    let days_diff = time_diff / 86400.0;
                    let half_life_days = 30.0;
                    let decay_factor = 0.5_f64.powf(days_diff / half_life_days);
                    current_score * decay_factor + 1.0
                } else {
                    1.0
                };

                // Update the project
                self.conn.execute(
                    "UPDATE projects SET frecency_score = ?1, last_accessed = ?2, access_count = ?3 WHERE id = ?4",
                    params![new_score, now, access_count + 1, id],
                )?;

                Ok(id)
            },
        ).optional()?;

        Ok(project_id.is_some())
    }

    /// Get projects sorted by frecency score
    pub fn get_projects_by_frecency(&self, limit: usize) -> Result<Vec<Project>> {
        // Use a JOIN to fetch indicators in a single query (avoid N+1)
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.path, p.project_type, p.last_scanned, pi.indicator_type, p.frecency_score
             FROM projects p
             LEFT JOIN project_indicators pi ON p.id = pi.project_id
             WHERE p.frecency_score > 0
             ORDER BY p.frecency_score DESC, p.path, pi.indicator_type
             LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            let project_type_json: String = row.get(2)?;
            let last_scanned_str: String = row.get(3)?;
            let indicator_json: Option<String> = row.get(4)?;

            Ok((
                row.get::<_, i64>(0)?, // id
                row.get::<_, String>(1)?, // path
                serde_json::from_str(&project_type_json).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?,
                chrono::DateTime::parse_from_rfc3339(&last_scanned_str).map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?.with_timezone(&chrono::Utc),
                indicator_json,
            ))
        })?;

        // Group results by project (since JOIN creates multiple rows per project)
        let mut projects_map: std::collections::HashMap<String, (i64, ProjectType, chrono::DateTime<chrono::Utc>, Vec<ProjectIndicator>)> = std::collections::HashMap::new();
        let mut project_order: Vec<String> = Vec::new();

        for row_result in rows {
            let (id, path_str, project_type, last_scanned, indicator_json) = row_result?;

            if !projects_map.contains_key(&path_str) {
                project_order.push(path_str.clone());
                projects_map.insert(path_str.clone(), (id, project_type, last_scanned, Vec::new()));
            }

            if let Some(ind_json) = indicator_json {
                let indicator: ProjectIndicator = serde_json::from_str(&ind_json)
                    .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
                if let Some(entry) = projects_map.get_mut(&path_str) {
                    entry.3.push(indicator);
                }
            }

            // Respect the limit on number of projects (not rows)
            if projects_map.len() >= limit {
                break;
            }
        }

        // Maintain frecency order
        let projects: Vec<Project> = project_order
            .into_iter()
            .filter_map(|path_str| {
                projects_map.remove(&path_str).map(|(_, project_type, last_scanned, indicators)| Project {
                    path: std::path::PathBuf::from(path_str),
                    project_type,
                    indicators,
                    last_scanned,
                })
            })
            .collect();

        Ok(projects)
    }

    /// Get frecency score for a specific path
    pub fn get_frecency_score<P: AsRef<Path>>(&self, path: P) -> Result<Option<f64>> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let now = chrono::Utc::now().timestamp();

        let score = self.conn.query_row(
            "SELECT frecency_score, last_accessed FROM projects WHERE path = ?1",
            params![path_str],
            |row| {
                let current_score: f64 = row.get(0)?;
                let last_accessed: Option<i64> = row.get(1)?;

                // Apply time decay
                let decayed_score = if let Some(last_access) = last_accessed {
                    let time_diff = (now - last_access) as f64;
                    let days_diff = time_diff / 86400.0;
                    let half_life_days = 30.0;
                    let decay_factor = 0.5_f64.powf(days_diff / half_life_days);
                    current_score * decay_factor
                } else {
                    current_score
                };

                Ok(decayed_score)
            },
        ).optional()?;

        Ok(score)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use dprojc_types::ScanErrorType;


    fn create_test_db() -> Result<ProjectDatabase> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!("dprojc_test_{}_{}", std::process::id(), count));
        std::fs::create_dir_all(&temp_dir)?;
        let db_path = temp_dir.join("test.db");
        ProjectDatabase::open(&db_path)
    }

    #[test]
    fn test_open_database() {
        let db = create_test_db();
        assert!(db.is_ok());
    }

    #[test]
    fn test_schema_initialization() -> Result<()> {
        let db = create_test_db()?;

        // Check that tables exist
        let tables: Vec<String> = db.conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, rusqlite::Error>>()
            .unwrap();

        assert!(tables.contains(&"projects".to_string()));
        assert!(tables.contains(&"project_indicators".to_string()));
        assert!(tables.contains(&"scan_results".to_string()));
        assert!(tables.contains(&"scan_errors".to_string()));
        assert!(tables.contains(&"excluded_dirs".to_string()));
        assert!(tables.contains(&"scan_projects".to_string()));
        assert!(tables.contains(&"schema_version".to_string()));
        assert!(tables.contains(&"migrations".to_string()));
        Ok(())
    }

    #[test]
    fn test_schema_version() {
        let db = create_test_db().unwrap();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_upsert_and_get_project() -> Result<()> {
        let mut db = create_test_db()?;
        let project = Project {
            path: std::path::PathBuf::from("/test/project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        // Insert project
        let project_id = db.upsert_project(&project)?;
        assert!(project_id > 0);

        // Get project by path
        let retrieved = db.get_project_by_path("/test/project")?;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.path, project.path);
        assert_eq!(retrieved.project_type, project.project_type);
        assert_eq!(retrieved.indicators, project.indicators);

        Ok(())
    }

    #[test]
    fn test_get_all_projects() -> Result<()> {
        let mut db = create_test_db()?;

        let projects = vec![
            Project {
                path: std::path::PathBuf::from("/test/project1"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: std::path::PathBuf::from("/test/project2"),
                project_type: ProjectType::NodeJs,
                indicators: vec![ProjectIndicator::PackageJson],
                last_scanned: chrono::Utc::now(),
            },
        ];

        for project in &projects {
            db.upsert_project(project)?;
        }

        let all_projects = db.get_all_projects()?;
        assert_eq!(all_projects.len(), 2);

        Ok(())
    }

    #[test]
    fn test_get_projects_by_type() -> Result<()> {
        let mut db = create_test_db()?;

        let rust_project = Project {
            path: std::path::PathBuf::from("/test/rust_project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        let node_project = Project {
            path: std::path::PathBuf::from("/test/node_project"),
            project_type: ProjectType::NodeJs,
            indicators: vec![ProjectIndicator::PackageJson],
            last_scanned: chrono::Utc::now(),
        };

        db.upsert_project(&rust_project)?;
        db.upsert_project(&node_project)?;

        let rust_projects = db.get_projects_by_type(&ProjectType::Rust)?;
        assert_eq!(rust_projects.len(), 1);
        assert_eq!(rust_projects[0].project_type, ProjectType::Rust);

        let node_projects = db.get_projects_by_type(&ProjectType::NodeJs)?;
        assert_eq!(node_projects.len(), 1);
        assert_eq!(node_projects[0].project_type, ProjectType::NodeJs);

        Ok(())
    }

    #[test]
    fn test_search_projects_by_path() -> Result<()> {
        let mut db = create_test_db()?;

        let project = Project {
            path: std::path::PathBuf::from("/home/user/my_project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        db.upsert_project(&project)?;

        let results = db.search_projects_by_path("my_project")?;
        assert_eq!(results.len(), 1);

        let results = db.search_projects_by_path("nonexistent")?;
        assert_eq!(results.len(), 0);

        Ok(())
    }

    #[test]
    fn test_store_and_get_scan_result() -> Result<()> {
        let mut db = create_test_db()?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/root"),
            projects: vec![Project {
                path: std::path::PathBuf::from("/test/project"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            }],
            excluded_dirs: vec![std::path::PathBuf::from("/test/excluded")],
            errors: vec![ScanError {
                path: std::path::PathBuf::from("/test/error_path"),
                error_type: ScanErrorType::PermissionDenied,
                message: "Permission denied".to_string(),
            }],
            dirs_scanned: 100,
            scan_duration_ms: 5000,
        };

        // Store scan result
        let scan_id = db.store_scan_result(&scan_result)?;
        assert!(scan_id > 0);

        // Get scan result
        let retrieved = db.get_scan_result(scan_id)?;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.root_path, scan_result.root_path);
        assert_eq!(retrieved.projects.len(), 1);
        assert_eq!(retrieved.excluded_dirs, scan_result.excluded_dirs);
        assert_eq!(retrieved.errors.len(), 1);
        assert_eq!(retrieved.dirs_scanned, scan_result.dirs_scanned);

        Ok(())
    }

    #[test]
    fn test_get_recent_scan_results() -> Result<()> {
        let mut db = create_test_db()?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/root"),
            projects: vec![],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 50,
            scan_duration_ms: 2000,
        };

        db.store_scan_result(&scan_result)?;

        let summaries = db.get_recent_scan_results(10)?;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].dirs_scanned, 50);

        Ok(())
    }

    #[test]
    fn test_get_scan_statistics() -> Result<()> {
        let mut db = create_test_db()?;

        let stats = db.get_scan_statistics()?;
        assert_eq!(stats.total_scans, 0);

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/root"),
            projects: vec![Project {
                path: std::path::PathBuf::from("/test/project"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            }],
            excluded_dirs: vec![],
            errors: vec![ScanError {
                path: std::path::PathBuf::from("/test/error"),
                error_type: ScanErrorType::IoError,
                message: "IO error".to_string(),
            }],
            dirs_scanned: 25,
            scan_duration_ms: 1000,
        };

        db.store_scan_result(&scan_result)?;

        let stats = db.get_scan_statistics()?;
        assert_eq!(stats.total_scans, 1);
        assert_eq!(stats.total_projects, 1);
        assert_eq!(stats.total_dirs_scanned, 25);
        assert_eq!(stats.total_errors, 1);
        assert!(stats.last_scan_timestamp.is_some());

        Ok(())
    }

    #[test]
    fn test_delete_project() -> Result<()> {
        let mut db = create_test_db()?;

        let project = Project {
            path: std::path::PathBuf::from("/test/project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        db.upsert_project(&project)?;

        // Verify project exists
        let retrieved = db.get_project_by_path("/test/project")?;
        assert!(retrieved.is_some());

        // Delete project
        let deleted = db.delete_project_by_path("/test/project")?;
        assert!(deleted);

        // Verify project is gone
        let retrieved = db.get_project_by_path("/test/project")?;
        assert!(retrieved.is_none());

        Ok(())
    }

    #[test]
    fn test_get_projects_by_indicator() -> Result<()> {
        let mut db = create_test_db()?;

        let rust_project = Project {
            path: std::path::PathBuf::from("/test/rust_project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        let node_project = Project {
            path: std::path::PathBuf::from("/test/node_project"),
            project_type: ProjectType::NodeJs,
            indicators: vec![ProjectIndicator::PackageJson],
            last_scanned: chrono::Utc::now(),
        };

        db.upsert_project(&rust_project)?;
        db.upsert_project(&node_project)?;

        let cargo_projects = db.get_projects_by_indicator(&ProjectIndicator::CargoToml)?;
        assert_eq!(cargo_projects.len(), 1);
        assert_eq!(cargo_projects[0].project_type, ProjectType::Rust);

        let package_projects = db.get_projects_by_indicator(&ProjectIndicator::PackageJson)?;
        assert_eq!(package_projects.len(), 1);
        assert_eq!(package_projects[0].project_type, ProjectType::NodeJs);

        Ok(())
    }

    #[test]
    fn test_get_project_counts_by_type() -> Result<()> {
        let mut db = create_test_db()?;

        let rust_project1 = Project {
            path: std::path::PathBuf::from("/test/rust1"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        let rust_project2 = Project {
            path: std::path::PathBuf::from("/test/rust2"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };

        let node_project = Project {
            path: std::path::PathBuf::from("/test/node"),
            project_type: ProjectType::NodeJs,
            indicators: vec![ProjectIndicator::PackageJson],
            last_scanned: chrono::Utc::now(),
        };

        db.upsert_project(&rust_project1)?;
        db.upsert_project(&rust_project2)?;
        db.upsert_project(&node_project)?;

        let counts = db.get_project_counts_by_type()?;
        assert_eq!(counts.get(&ProjectType::Rust), Some(&2));
        assert_eq!(counts.get(&ProjectType::NodeJs), Some(&1));

        Ok(())
    }

    #[test]
    fn test_backup_and_restore() -> Result<()> {
        let mut db = create_test_db()?;

        // Add some test data
        let project = Project {
            path: std::path::PathBuf::from("/test/backup_project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };
        db.upsert_project(&project)?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/backup_scan"),
            projects: vec![project],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 10,
            scan_duration_ms: 100,
        };
        db.store_scan_result(&scan_result)?;

        // Backup
        let backup_path = std::env::temp_dir().join("test_backup.sql");
        db.backup_to_sql(&backup_path)?;
        assert!(backup_path.exists());

        // Create new database and restore
        let restore_db_path = std::env::temp_dir().join("test_restore.db");
        let restore_db = ProjectDatabase::open(&restore_db_path)?;
        restore_db.restore_from_sql(&backup_path)?;

        // Verify data was restored
        let restored_projects = restore_db.get_all_projects()?;
        assert_eq!(restored_projects.len(), 1);

        let restored_stats = restore_db.get_scan_statistics()?;
        assert_eq!(restored_stats.total_scans, 1);
        assert_eq!(restored_stats.total_projects, 1);

        // Clean up
        std::fs::remove_file(&backup_path)?;
        std::fs::remove_file(&restore_db_path)?;

        Ok(())
    }

    #[test]
    fn test_has_path_been_scanned_recently() -> Result<()> {
        let mut db = create_test_db()?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/path"),
            projects: vec![],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 5,
            scan_duration_ms: 50,
        };

        // Store scan result
        db.store_scan_result(&scan_result)?;

        // Check if path has been scanned recently (within 1 hour)
        let recent = db.has_path_been_scanned_recently("/test/path", chrono::Utc::now() - chrono::Duration::hours(1))?;
        assert!(recent);

        // Check if path has been scanned in the far past (should be false)
        let not_recent = db.has_path_been_scanned_recently("/test/path", chrono::Utc::now() + chrono::Duration::hours(1))?;
        assert!(!not_recent);

        // Check non-existent path
        let nonexistent = db.has_path_been_scanned_recently("/nonexistent", chrono::Utc::now() - chrono::Duration::hours(1))?;
        assert!(!nonexistent);

        Ok(())
    }

    #[test]
    fn test_delete_old_scan_results() -> Result<()> {
        let mut db = create_test_db()?;

        // Create scan results
        let scan1 = ScanResult {
            root_path: std::path::PathBuf::from("/scan1"),
            projects: vec![],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 1,
            scan_duration_ms: 10,
        };

        let scan2 = ScanResult {
            root_path: std::path::PathBuf::from("/scan2"),
            projects: vec![],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 1,
            scan_duration_ms: 10,
        };

        // Store both
        db.store_scan_result(&scan1)?;
        db.store_scan_result(&scan2)?;

        // Verify both exist
        let summaries = db.get_recent_scan_results(10)?;
        assert_eq!(summaries.len(), 2);

        // Delete scans older than now (both should be deleted since they were created before now)
        let deleted = db.delete_old_scan_results(chrono::Utc::now())?;
        assert_eq!(deleted, 2);

        // Verify none remain
        let summaries = db.get_recent_scan_results(10)?;
        assert_eq!(summaries.len(), 0);

        Ok(())
    }

    #[test]
    fn test_cleanup_orphaned_projects() -> Result<()> {
        let mut db = create_test_db()?;

        // Create a project
        let project = Project {
            path: std::path::PathBuf::from("/test/project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };
        db.upsert_project(&project)?;

        // Create a scan result that includes the project
        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/scan"),
            projects: vec![project],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 1,
            scan_duration_ms: 10,
        };
        db.store_scan_result(&scan_result)?;

        // Verify project exists
        let projects = db.get_all_projects()?;
        assert_eq!(projects.len(), 1);

        // Cleanup orphaned projects with cutoff 1 day (should not delete since scan is recent)
        let cleaned = db.cleanup_orphaned_projects(1)?;
        assert_eq!(cleaned, 0);

        // Delete the scan result
        let summaries = db.get_recent_scan_results(10)?;
        db.delete_scan_result(summaries[0].id)?;

        // Now cleanup should delete the project since it's not linked to any recent scans
        let cleaned = db.cleanup_orphaned_projects(1)?;
        assert_eq!(cleaned, 1);

        // Verify project is gone
        let projects = db.get_all_projects()?;
        assert_eq!(projects.len(), 0);

        Ok(())
    }

    #[test]
    fn test_clear_all_data() -> Result<()> {
        let mut db = create_test_db()?;

        // Add data
        let project = Project {
            path: std::path::PathBuf::from("/test/project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };
        db.upsert_project(&project)?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/scan"),
            projects: vec![project],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 1,
            scan_duration_ms: 10,
        };
        db.store_scan_result(&scan_result)?;

        // Verify data exists
        let projects = db.get_all_projects()?;
        assert_eq!(projects.len(), 1);
        let stats = db.get_scan_statistics()?;
        assert_eq!(stats.total_scans, 1);

        // Clear all data
        db.clear_all_data()?;

        // Verify data is gone
        let projects = db.get_all_projects()?;
        assert_eq!(projects.len(), 0);
        let stats = db.get_scan_statistics()?;
        assert_eq!(stats.total_scans, 0);

        Ok(())
    }

    #[test]
    fn test_restore_from_invalid_sql() {
        let db = create_test_db().unwrap();

        // Try to restore from invalid SQL
        let result = db.restore_from_sql(std::path::Path::new("/nonexistent/file.sql"));
        assert!(result.is_err());

        // Try to restore from empty file
        let temp_file = std::env::temp_dir().join("empty.sql");
        std::fs::write(&temp_file, "").unwrap();
        let _result = db.restore_from_sql(&temp_file);
        // This might succeed or fail depending on SQL, but should not panic
        std::fs::remove_file(&temp_file).unwrap();
    }

    #[test]
    fn test_edge_cases() -> Result<()> {
        let mut db = create_test_db()?;

        // Test project with no indicators
        let project_no_indicators = Project {
            path: std::path::PathBuf::from("/test/no_indicators"),
            project_type: ProjectType::Unknown,
            indicators: vec![],
            last_scanned: chrono::Utc::now(),
        };
        db.upsert_project(&project_no_indicators)?;
        let retrieved = db.get_project_by_path("/test/no_indicators")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().indicators.len(), 0);

        // Test project with special characters in path
        let project_special = Project {
            path: std::path::PathBuf::from("/test/path with spaces & symbols!@#"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        };
        db.upsert_project(&project_special)?;
        let retrieved = db.get_project_by_path("/test/path with spaces & symbols!@#")?;
        assert!(retrieved.is_some());

        // Test search with special characters
        let results = db.search_projects_by_path("spaces")?;
        assert_eq!(results.len(), 1);

        // Test empty search
        let results = db.search_projects_by_path("")?;
        assert_eq!(results.len(), 2); // Should match all

        Ok(())
    }

    #[test]
    fn test_delete_scan_result() -> Result<()> {
        let mut db = create_test_db()?;

        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/root"),
            projects: vec![],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 10,
            scan_duration_ms: 500,
        };

        let scan_id = db.store_scan_result(&scan_result)?;

        // Verify scan exists
        let summaries = db.get_recent_scan_results(10)?;
        assert_eq!(summaries.len(), 1);

        // Delete scan
        let deleted = db.delete_scan_result(scan_id)?;
        assert!(deleted);

        // Verify scan is gone
        let summaries = db.get_recent_scan_results(10)?;
        assert_eq!(summaries.len(), 0);

        Ok(())
    }

}
