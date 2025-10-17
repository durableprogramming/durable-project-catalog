use dprojc_config::ConfigManager;
use dprojc_db::ProjectDatabase;
use dprojc_types::{ScanResultSummary, ScanStatistics};
use dprojc_scanner::{scan_directory_with_config, SharedScanner};
use dprojc_types::{Project, ProjectType, ScanConfig, ScanResult, ProjectIndicator};
use dprojc_utils::default_db_path;
use std::path::Path;


/// Main catalog struct that orchestrates scanning, database operations, and configuration
pub struct ProjectCatalog {
    config: ScanConfig,
    db: ProjectDatabase,
    scanner: SharedScanner,
}

impl ProjectCatalog {
    /// Create a new catalog with default configuration and database
    pub async fn new() -> anyhow::Result<Self> {
        let config = ConfigManager::load_config()?;
        let db_path = default_db_path()?;
        let db = ProjectDatabase::open(&db_path)?;
        let scanner = SharedScanner::with_config(config.clone());

        Ok(Self { config, db, scanner })
    }

    /// Create a new catalog with custom configuration
    pub async fn with_config(config: ScanConfig) -> anyhow::Result<Self> {
        let db_path = default_db_path()?;
        let db = ProjectDatabase::open(&db_path)?;
        // Validate config before creating scanner
        dprojc_utils::validate_scan_config(&config)?;
        let scanner = SharedScanner::with_config(config.clone());

        Ok(Self { config, db, scanner })
    }

    /// Create a new catalog with custom database path
    pub async fn with_db_path<P: AsRef<Path>>(db_path: P) -> anyhow::Result<Self> {
        let config = ConfigManager::load_config()?;
        let db = ProjectDatabase::open(db_path)?;
        let scanner = SharedScanner::with_config(config.clone());

        Ok(Self { config, db, scanner })
    }

    /// Scan a single directory for projects and store results
    pub async fn scan_directory<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<ScanResult> {
        let scan_result = self.scanner.scan(path.as_ref()).await?;
        self.db.store_scan_result(&scan_result)?;
        Ok(scan_result)
    }

    /// Scan multiple directories concurrently and store results
    pub async fn scan_directories(&mut self, paths: &[std::path::PathBuf]) -> anyhow::Result<Vec<ScanResult>> {
        let scan_results = self.scanner.scan_multiple(paths).await?;
        for result in &scan_results {
            self.db.store_scan_result(result)?;
        }
        Ok(scan_results)
    }

    /// Get all projects from the database
    pub async fn get_all_projects(&self) -> anyhow::Result<Vec<Project>> {
        Ok(self.db.get_all_projects()?)
    }

    /// Get projects by type
    pub async fn get_projects_by_type(&self, project_type: &ProjectType) -> anyhow::Result<Vec<Project>> {
        Ok(self.db.get_projects_by_type(project_type)?)
    }

    /// Search projects by path pattern
    pub async fn search_projects(&self, pattern: &str) -> anyhow::Result<Vec<Project>> {
        Ok(self.db.search_projects_by_path(pattern)?)
    }

    /// Get projects by indicator type
    pub async fn get_projects_by_indicator(&self, indicator: &dprojc_types::ProjectIndicator) -> anyhow::Result<Vec<Project>> {
        Ok(self.db.get_projects_by_indicator(indicator)?)
    }

    /// Get project counts by type
    pub async fn get_project_counts(&self) -> anyhow::Result<std::collections::HashMap<ProjectType, usize>> {
        Ok(self.db.get_project_counts_by_type()?)
    }

    /// Get recent scan results
    pub async fn get_recent_scans(&self, limit: usize) -> anyhow::Result<Vec<ScanResultSummary>> {
        Ok(self.db.get_recent_scan_results(limit)?)
    }

    /// Get scan statistics
    pub async fn get_scan_statistics(&self) -> anyhow::Result<ScanStatistics> {
        Ok(self.db.get_scan_statistics()?)
    }

    /// Get a project by path
    pub async fn get_project_by_path<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<Option<Project>> {
        Ok(self.db.get_project_by_path(path)?)
    }

    /// Delete a project by path
    pub async fn delete_project_by_path<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<bool> {
        Ok(self.db.delete_project_by_path(path)?)
    }

    /// Get the current configuration
    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Update the configuration
    pub async fn set_config(&mut self, config: ScanConfig) {
        self.config = config.clone();
        self.scanner.set_config(config).await;
    }

    /// Perform an incremental scan (only scan paths that haven't been scanned recently)
    pub async fn incremental_scan(&mut self, paths: &[std::path::PathBuf], max_age_hours: i64) -> anyhow::Result<Vec<ScanResult>> {
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(max_age_hours);
        let mut results = Vec::new();

        for path in paths {
            // Check if this path has been scanned recently
            let has_recent_scan = self.db.has_path_been_scanned_recently(path, cutoff_time)?;

            if !has_recent_scan {
                let result = self.scan_directory(path).await?;
                results.push(result);
            }
        }

        Ok(results)
    }

    /// Export projects to JSON
    pub async fn export_projects_json(&self) -> anyhow::Result<String> {
        let projects = self.get_all_projects().await?;
        serde_json::to_string_pretty(&projects).map_err(Into::into)
    }

    /// Export scan statistics to JSON
    pub async fn export_statistics_json(&self) -> anyhow::Result<String> {
        let stats = self.get_scan_statistics().await?;
        serde_json::to_string_pretty(&stats).map_err(Into::into)
    }

    /// Clean up old scan results
    pub async fn clean_old_scans(&self, max_age_days: i64) -> anyhow::Result<usize> {
        let cutoff_time = chrono::Utc::now() - chrono::Duration::days(max_age_days);
        Ok(self.db.delete_old_scan_results(cutoff_time)?)
    }

    /// Clean up orphaned projects (projects not linked to recent scans)
    pub async fn cleanup_orphaned_projects(&self, max_age_days: i64) -> anyhow::Result<usize> {
        Ok(self.db.cleanup_orphaned_projects(max_age_days)?)
    }

    /// Clear all data (for testing or reset)
    pub async fn clear_all_data(&self) -> anyhow::Result<()> {
        Ok(self.db.clear_all_data()?)
    }

    /// Record access to a project for frecency tracking
    pub async fn record_project_access<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<bool> {
        Ok(self.db.record_access(path)?)
    }

    /// Get projects sorted by frecency score
    pub async fn get_projects_by_frecency(&self, limit: usize) -> anyhow::Result<Vec<Project>> {
        Ok(self.db.get_projects_by_frecency(limit)?)
    }

    /// Get frecency score for a specific project path
    pub async fn get_project_frecency_score<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<Option<f64>> {
        Ok(self.db.get_frecency_score(path)?)
    }

    /// Backup database to SQL file
    pub async fn backup_database<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        Ok(self.db.backup_to_sql(path)?)
    }

    /// Restore database from SQL file
    pub async fn restore_database<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        Ok(self.db.restore_from_sql(path)?)
    }
}

impl Default for ProjectCatalog {
    fn default() -> Self {
        // Note: This will panic if async, but Default is sync.
        // Users should use new() instead.
        panic!("Use ProjectCatalog::new() instead of default()")
    }
}

/// Utility functions for one-off operations
pub mod utils {
    use super::*;

    /// Scan a directory and return results without storing in database
    pub async fn scan_directory_once<P: AsRef<Path>>(path: P) -> anyhow::Result<ScanResult> {
        let config = ConfigManager::load_config()?;
        scan_directory_with_config(path.as_ref(), config).await
    }

    /// Scan a directory with custom config without storing in database
    pub async fn scan_directory_with_custom_config<P: AsRef<Path>>(path: P, config: ScanConfig) -> anyhow::Result<ScanResult> {
        scan_directory_with_config(path.as_ref(), config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::tempdir;

    async fn create_test_catalog() -> (ProjectCatalog, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let _config = ScanConfig::default();
        let catalog = ProjectCatalog::with_db_path(db_path).await.unwrap();
        (catalog, temp_dir)
    }

    #[tokio::test]
    async fn test_catalog_creation() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        assert!(catalog.config().max_depth.is_some());
    }

    #[tokio::test]
    async fn test_scan_and_retrieve_projects() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Scan directory
        let scan_result = catalog.scan_directory(temp_dir.path()).await.unwrap();
        assert_eq!(scan_result.projects.len(), 1);

        // Retrieve projects
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_type, ProjectType::Git);
    }

    #[tokio::test]
    async fn test_get_projects_by_type() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();

        fs::create_dir(temp_dir1.path().join(".git")).unwrap();
        fs::write(temp_dir2.path().join("Cargo.toml"), "[package]").unwrap();

        catalog.scan_directories(&[temp_dir1.path().to_path_buf(), temp_dir2.path().to_path_buf()]).await.unwrap();

        let git_projects = catalog.get_projects_by_type(&ProjectType::Git).await.unwrap();
        let rust_projects = catalog.get_projects_by_type(&ProjectType::Rust).await.unwrap();

        assert_eq!(git_projects.len(), 1);
        assert_eq!(rust_projects.len(), 1);
    }

    #[tokio::test]
    async fn test_search_projects() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let results = catalog.search_projects("tmp").await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_project_counts() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();

        fs::create_dir(temp_dir1.path().join(".git")).unwrap();
        fs::write(temp_dir2.path().join("Cargo.toml"), "[package]").unwrap();

        catalog.scan_directories(&[temp_dir1.path().to_path_buf(), temp_dir2.path().to_path_buf()]).await.unwrap();

        let counts = catalog.get_project_counts().await.unwrap();
        assert_eq!(*counts.get(&ProjectType::Git).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&ProjectType::Rust).unwrap_or(&0), 1);
    }

    #[tokio::test]
    async fn test_scan_statistics() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let stats = catalog.get_scan_statistics().await.unwrap();
        assert_eq!(stats.total_scans, 1);
        assert_eq!(stats.total_projects, 1);
        assert!(stats.last_scan_timestamp.is_some());
    }

    #[tokio::test]
    async fn test_incremental_scan() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // First scan
        let results1 = catalog.incremental_scan(&[temp_dir.path().to_path_buf()], 1).await.unwrap();
        assert_eq!(results1.len(), 1);

        // Second scan within the hour should be skipped
        let results2 = catalog.incremental_scan(&[temp_dir.path().to_path_buf()], 1).await.unwrap();
        assert_eq!(results2.len(), 0);
    }

    #[tokio::test]
    async fn test_export_projects_json() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let json = catalog.export_projects_json().await.unwrap();
        assert!(json.contains("Git"));
        assert!(json.contains("path"));
    }

    #[tokio::test]
    async fn test_delete_project() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        // Verify project exists
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 1);

        // Delete project
        let deleted = catalog.delete_project_by_path(temp_dir.path()).await.unwrap();
        assert!(deleted);

        // Verify project is gone
        let projects_after = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects_after.len(), 0);
    }

    #[tokio::test]
    async fn test_utils_scan_directory_once() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let result = utils::scan_directory_once(temp_dir.path()).await.unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Git);
    }

    #[tokio::test]
    async fn test_error_handling_invalid_path() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let invalid_path = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let result = catalog.scan_directory(invalid_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_multiple_with_mixed_valid_invalid_paths() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let valid_path = temp_dir.path().to_path_buf();
        let invalid_path = std::path::PathBuf::from("/nonexistent/path");

        let results = catalog.scan_directories(&[valid_path, invalid_path]).await;
        assert!(results.is_err()); // Should fail due to invalid path
    }

    #[tokio::test]
    async fn test_get_project_by_path_nonexistent() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        let result = catalog.get_project_by_path("/nonexistent/project").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_project_by_path_nonexistent() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        let deleted = catalog.delete_project_by_path("/nonexistent/project").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_empty_project_catalog_operations() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        // Test operations on empty catalog
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 0);

        let counts = catalog.get_project_counts().await.unwrap();
        assert!(counts.is_empty());

        let stats = catalog.get_scan_statistics().await.unwrap();
        assert_eq!(stats.total_scans, 0);
        assert_eq!(stats.total_projects, 0);
        assert_eq!(stats.total_dirs_scanned, 0);
        assert_eq!(stats.total_errors, 0);
        assert!(stats.last_scan_timestamp.is_none());
    }

    #[tokio::test]
    async fn test_config_update_and_scanner_reuse() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Scan with default config
        let result1 = catalog.scan_directory(temp_dir.path()).await.unwrap();
        assert_eq!(result1.projects.len(), 1);

        // Update config to limit depth
        let mut new_config = catalog.config().clone();
        new_config.max_depth = Some(0); // Only scan root
        catalog.set_config(new_config).await;

        // Create a new temp dir with nested project
        let temp_dir2 = tempdir().unwrap();
        let nested_dir = temp_dir2.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::create_dir(nested_dir.join(".git")).unwrap();

        // Scan again - should not find the nested project due to depth limit
        let result2 = catalog.scan_directory(temp_dir2.path()).await.unwrap();
        assert_eq!(result2.projects.len(), 0);
    }

    #[tokio::test]
    async fn test_incremental_scan_edge_cases() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        let scan_path = temp_dir.path().to_path_buf();

        // Test with empty path list
        let results = catalog.incremental_scan(&[], 1).await.unwrap();
        assert_eq!(results.len(), 0);

        // First scan
        let results1 = catalog.incremental_scan(&[scan_path.clone()], 1).await.unwrap();
        assert_eq!(results1.len(), 1);

        // Check that scan was recorded
        let stats = catalog.get_scan_statistics().await.unwrap();
        assert_eq!(stats.total_scans, 1);

        // Check recent scans
        let summaries = catalog.get_recent_scans(10).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].root_path, scan_path);

        // Test the database method directly
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(1);
        let has_recent = catalog.db.has_path_been_scanned_recently(&scan_path, cutoff_time).unwrap();
        assert!(has_recent, "Path should have been scanned recently");

        // Scan again with a cutoff that should prevent scanning (since scan was recent)
        let results2 = catalog.incremental_scan(&[scan_path.clone()], 1).await.unwrap();
        assert_eq!(results2.len(), 0);

        // Scan with very recent cutoff (0 hours) - should scan again since scan_timestamp <= now
        let results3 = catalog.incremental_scan(&[scan_path], 0).await.unwrap();
        assert_eq!(results3.len(), 1);
    }

    #[tokio::test]
    async fn test_export_operations_on_empty_catalog() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        let json_projects = catalog.export_projects_json().await.unwrap();
        assert!(json_projects.contains("[]")); // Empty array

        let json_stats = catalog.export_statistics_json().await.unwrap();
        let stats: ScanStatistics = serde_json::from_str(&json_stats).unwrap();
        assert_eq!(stats.total_scans, 0);
    }

    #[tokio::test]
    async fn test_cleanup_operations() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        // Test cleanup with large age (should not delete anything in empty DB)
        let cleaned = catalog.clean_old_scans(365).await.unwrap();
        assert_eq!(cleaned, 0);

        let orphaned = catalog.cleanup_orphaned_projects(365).await.unwrap();
        assert_eq!(orphaned, 0);
    }

    #[tokio::test]
    async fn test_concurrent_scans() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();
        fs::create_dir(temp_dir1.path().join(".git")).unwrap();
        fs::write(temp_dir2.path().join("Cargo.toml"), "[package]").unwrap();

        // Create multiple catalog instances to test concurrent access
        let mut catalog1 = ProjectCatalog::with_db_path(_temp_dir.path().join("test.db")).await.unwrap();
        let mut catalog2 = ProjectCatalog::with_db_path(_temp_dir.path().join("test.db")).await.unwrap();

        // Scan concurrently
        let (result1, result2) = tokio::join!(
            catalog1.scan_directory(temp_dir1.path()),
            catalog2.scan_directory(temp_dir2.path())
        );

        result1.unwrap();
        result2.unwrap();

        // Verify both projects were stored
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 2);
    }

    #[tokio::test]
    async fn test_frecency_operations() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Scan project
        catalog.scan_directory(temp_dir.path()).await.unwrap();

        // Record access
        let recorded = catalog.record_project_access(temp_dir.path()).await.unwrap();
        assert!(recorded);

        // Get frecency score
        let score = catalog.get_project_frecency_score(temp_dir.path()).await.unwrap();
        assert!(score.is_some());
        assert!(score.unwrap() > 0.0);

        // Get projects by frecency
        let frecency_projects = catalog.get_projects_by_frecency(10).await.unwrap();
        assert_eq!(frecency_projects.len(), 1);

        // Test non-existent project
        let nonexistent_score = catalog.get_project_frecency_score("/nonexistent").await.unwrap();
        assert!(nonexistent_score.is_none());

        let nonexistent_recorded = catalog.record_project_access("/nonexistent").await.unwrap();
        assert!(!nonexistent_recorded);
    }

    #[tokio::test]
    async fn test_backup_restore_operations() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Add some data
        catalog.scan_directory(temp_dir.path()).await.unwrap();

        // Backup
        let backup_path = _temp_dir.path().join("backup.sql");
        catalog.backup_database(&backup_path).await.unwrap();
        assert!(backup_path.exists());

        // Clear data
        catalog.clear_all_data().await.unwrap();
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 0);

        // Restore
        catalog.restore_database(&backup_path).await.unwrap();

        // Verify data was restored
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn test_database_creation_with_custom_path() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("custom.db");

        let catalog = ProjectCatalog::with_db_path(&db_path).await.unwrap();

        // Verify database was created
        assert!(db_path.exists());

        // Test basic operation
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_project_types_in_scan() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();

        // Create multiple project types
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 1); // Should be one project with multiple indicators

        let project = &projects[0];
        assert_eq!(project.project_type, ProjectType::Rust); // Rust takes precedence over NodeJs
        assert!(project.indicators.contains(&ProjectIndicator::GitDirectory));
        assert!(project.indicators.contains(&ProjectIndicator::PackageJson));
        assert!(project.indicators.contains(&ProjectIndicator::CargoToml));
    }

    #[tokio::test]
    async fn test_scan_result_summary_operations() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Scan
        catalog.scan_directory(temp_dir.path()).await.unwrap();

        // Get recent scans
        let summaries = catalog.get_recent_scans(10).await.unwrap();
        assert_eq!(summaries.len(), 1);

        let summary = &summaries[0];
        assert!(summary.dirs_scanned >= 1); // At least the root dir
        assert!(summary.scan_duration_ms >= 0); // Duration should be non-negative
        assert_eq!(summary.error_count, 0);
    }

    #[tokio::test]
    async fn test_utils_scan_with_custom_config() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let mut config = ScanConfig::default();
        config.max_depth = Some(1);

        let result = utils::scan_directory_with_custom_config(temp_dir.path(), config).await.unwrap();
        assert_eq!(result.projects.len(), 1);
    }

    #[tokio::test]
    async fn test_catalog_with_invalid_config() {
        // Config validation happens in the scanner, not in core
        let invalid_config = ScanConfig {
            max_depth: Some(0), // Invalid
            ..ScanConfig::default()
        };

        // Should fail during scanner creation
        let result = ProjectCatalog::with_config(invalid_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_large_number_of_projects() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        // Create many projects
        let mut temp_dirs = Vec::new();
        let mut paths = Vec::new();
        for i in 0..10 { // Reduced to 10 to avoid temp dir issues
            let temp_dir = tempdir().unwrap();
            let project_dir = temp_dir.path().join(format!("project_{}", i));
            fs::create_dir(&project_dir).unwrap();
            fs::create_dir(project_dir.join(".git")).unwrap();
            paths.push(project_dir);
            temp_dirs.push(temp_dir); // Keep temp dirs alive
        }

        // Scan all
        let path_bufs: Vec<_> = paths.iter().map(|p| p.to_path_buf()).collect();
        let results = catalog.scan_directories(&path_bufs).await.unwrap();
        assert_eq!(results.len(), 10);

        // Verify all were stored
        let projects = catalog.get_all_projects().await.unwrap();
        assert_eq!(projects.len(), 10);

        // Test frecency operations on some projects
        for project in &projects[..5] {
            catalog.record_project_access(&project.path).await.unwrap();
        }

        // Test pagination-like behavior with limits
        let limited_projects = catalog.get_projects_by_frecency(5).await.unwrap();
        assert_eq!(limited_projects.len(), 5);
    }

    #[tokio::test]
    async fn test_search_edge_cases() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        let project_path = temp_dir.path().join("my_special_project");
        fs::create_dir(&project_path).unwrap();
        fs::create_dir(project_path.join(".git")).unwrap();

        catalog.scan_directory(&project_path).await.unwrap();

        // Test various search patterns
        let results = catalog.search_projects("my_special").await.unwrap();
        assert_eq!(results.len(), 1);

        let results = catalog.search_projects("nonexistent").await.unwrap();
        assert_eq!(results.len(), 0);

        let results = catalog.search_projects("").await.unwrap();
        assert_eq!(results.len(), 1); // Should match all

        let results = catalog.search_projects("PROJECT").await.unwrap();
        assert_eq!(results.len(), 1); // Case insensitive
    }

    #[tokio::test]
    async fn test_project_counts_comprehensive() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        // Create different project types
        let project_types = vec![
            (".git", ProjectType::Git),
            ("package.json", ProjectType::NodeJs),
            ("Cargo.toml", ProjectType::Rust),
            ("Gemfile", ProjectType::Ruby),
            ("pyproject.toml", ProjectType::Python),
        ];

        for (i, (indicator, _expected_type)) in project_types.iter().enumerate() {
            let temp_dir = tempdir().unwrap();
            let project_dir = temp_dir.path().join(format!("project_{}", i));
            fs::create_dir(&project_dir).unwrap();

            match *indicator {
                ".git" => fs::create_dir(project_dir.join(indicator)).unwrap(),
                _ => fs::write(project_dir.join(indicator), "content").unwrap(),
            }

            catalog.scan_directory(&project_dir).await.unwrap();
        }

        let counts = catalog.get_project_counts().await.unwrap();
        assert_eq!(counts.len(), 5);
        assert_eq!(counts.get(&ProjectType::Git), Some(&1));
        assert_eq!(counts.get(&ProjectType::NodeJs), Some(&1));
        assert_eq!(counts.get(&ProjectType::Rust), Some(&1));
        assert_eq!(counts.get(&ProjectType::Ruby), Some(&1));
        assert_eq!(counts.get(&ProjectType::Python), Some(&1));
    }
}
