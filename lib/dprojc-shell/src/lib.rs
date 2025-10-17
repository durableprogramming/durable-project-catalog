//! Shell integration for the Durable Project Catalog
//!
//! This crate provides zoxide-like functionality for changing directories
//! with intelligent autocomplete based on the project database.

use anyhow::{Context, Result};
use dprojc_db::ProjectDatabase;
use dprojc_types::{Project, ProjectIndicator, ProjectType};
use std::path::{Path, PathBuf};

mod completions;

pub use completions::{generate_completions, ShellType};

/// Shell integration manager
pub struct ShellIntegration {
    db: ProjectDatabase,
}

impl ShellIntegration {
    /// Create a new shell integration with the given database path
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db =
            ProjectDatabase::open(db_path.as_ref()).context("Failed to open project database")?;

        Ok(Self { db })
    }

    /// Query projects matching a search pattern
    /// Returns paths sorted by frecency score (most relevant first)
    pub fn query(&self, pattern: &str, limit: usize) -> Result<Vec<PathBuf>> {
        if pattern.is_empty() {
            // Return frecent projects if no pattern specified
            let frecent_projects = self
                .db
                .get_projects_by_frecency(limit)
                .context("Failed to get projects by frecency")?;
            return Ok(frecent_projects.into_iter().map(|p| p.path).collect());
        }

        // Get frecent projects first (limit to reasonable number for performance)
        let frecent_limit = std::cmp::min(limit * 10, 500);
        let frecent_projects = self
            .db
            .get_projects_by_frecency(frecent_limit)
            .context("Failed to get projects by frecency")?;

        // Filter frecent projects by pattern
        let pattern_lower = pattern.to_lowercase();
        let mut matching: Vec<_> = frecent_projects
            .into_iter()
            .filter(|p| self.matches_pattern(&p.path, &pattern_lower))
            .take(limit)
            .collect();

        // If we need more results, use SQL-based search (much faster than loading all projects)
        if matching.len() < limit {
            // Use database search with limit for better performance
            let search_limit = (limit - matching.len()) * 3; // Get more than needed for filtering
            let search_results = self
                .db
                .search_projects_by_path_limit(&pattern_lower, Some(search_limit))
                .context("Failed to search projects")?;

            // Filter out projects we already have
            let existing_paths: std::collections::HashSet<_> =
                matching.iter().map(|p| &p.path).collect();

            let additional: Vec<_> = search_results
                .into_iter()
                .filter(|p| !existing_paths.contains(&p.path))
                .take(limit - matching.len())
                .collect();

            matching.extend(additional);
        }

        Ok(matching.into_iter().map(|p| p.path).collect())
    }

    /// Check if a path matches a search pattern
    /// Supports substring matching and path component matching
    fn matches_pattern(&self, path: &Path, pattern_lower: &str) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        // Exact substring match
        if path_str.contains(pattern_lower) {
            return true;
        }

        // Match against individual path components (directories and filename)
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                if name
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(pattern_lower)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Record a directory access (for frecency tracking)
    /// Returns true if the path was a project root and was recorded
    pub fn record_access<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        self.db
            .record_access(path.as_ref())
            .context("Failed to record directory access")
    }

    /// Get the best match for a pattern
    pub fn best_match(&self, pattern: &str) -> Result<Option<PathBuf>> {
        let results = self.query(pattern, 1)?;
        Ok(results.into_iter().next())
    }

    /// Get all project paths (for shell completion)
    pub fn all_projects(&self) -> Result<Vec<PathBuf>> {
        let projects = self
            .db
            .get_all_projects()
            .context("Failed to get all projects")?;
        Ok(projects.into_iter().map(|p| p.path).collect())
    }

    /// Check if a path is a cataloged project root
    pub fn is_project_root<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        let project = self
            .db
            .get_project_by_path(path.as_ref())
            .context("Failed to check if path is project root")?;
        Ok(project.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::tempdir;

    fn create_test_db() -> Result<(ProjectDatabase, PathBuf)> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "dprojc_shell_test_{}_{}",
            std::process::id(),
            count
        ));
        std::fs::create_dir_all(&temp_dir)?;
        let db_path = temp_dir.join("test.db");
        let db = ProjectDatabase::open(&db_path)?;
        Ok((db, db_path))
    }

    fn create_test_project(path: &str, project_type: ProjectType) -> Project {
        Project {
            path: PathBuf::from(path),
            project_type,
            indicators: vec![ProjectIndicator::GitDirectory], // Simple indicator for testing
            last_scanned: chrono::Utc::now(),
        }
    }

    fn setup_test_data(db: &mut ProjectDatabase) -> Result<()> {
        // Create various test projects
        let projects = vec![
            create_test_project("/home/user/projects/rust-app", ProjectType::Rust),
            create_test_project("/home/user/projects/node-web", ProjectType::NodeJs),
            create_test_project("/home/user/projects/python-tool", ProjectType::Python),
            create_test_project("/work/company/go-service", ProjectType::Go),
            create_test_project("/work/company/java-api", ProjectType::Java),
            create_test_project("/personal/blog", ProjectType::NodeJs),
            create_test_project("/personal/scripts", ProjectType::Python),
        ];

        for project in projects {
            db.upsert_project(&project)?;
        }

        // Record some access to create frecency data
        db.record_access("/home/user/projects/rust-app")?;
        db.record_access("/home/user/projects/rust-app")?; // Access twice for higher frecency
        db.record_access("/home/user/projects/node-web")?;
        db.record_access("/personal/blog")?;

        Ok(())
    }

    #[test]
    fn test_shell_integration_creation() {
        let (db, db_path) = create_test_db().unwrap();
        let shell = ShellIntegration::new(&db_path);
        assert!(shell.is_ok());
    }

    #[test]
    fn test_query_empty_database() {
        let (db, db_path) = create_test_db().unwrap();
        let shell = ShellIntegration::new(&db_path).unwrap();
        let results = shell.query("test", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_query_with_data() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test exact substring match
        let results = shell.query("rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].to_string_lossy().contains("rust-app"));

        // Test case insensitive match
        let results = shell.query("RUST", 10).unwrap();
        assert_eq!(results.len(), 1);

        // Test path component match
        let results = shell.query("projects", 10).unwrap();
        assert!(results.len() >= 3); // Should match multiple projects in projects directory

        // Test limit
        let results = shell.query("project", 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_empty_pattern() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Empty pattern should return frecent projects
        let results = shell.query("", 3).unwrap();
        assert!(results.len() <= 3);

        // Should prioritize most accessed projects
        let rust_app_pos = results
            .iter()
            .position(|p| p.to_string_lossy().contains("rust-app"));
        assert!(
            rust_app_pos.is_some(),
            "Most accessed project should be in results"
        );
    }

    #[test]
    fn test_matches_pattern() {
        let (db, db_path) = create_test_db().unwrap();
        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test substring matching
        assert!(shell.matches_pattern(&PathBuf::from("/home/user/project"), "user"));
        assert!(shell.matches_pattern(&PathBuf::from("/home/user/project"), "project"));

        // Test path component matching
        assert!(shell.matches_pattern(&PathBuf::from("/home/user/my-project"), "my-project"));
        assert!(shell.matches_pattern(&PathBuf::from("/work/company/api"), "company"));

        // Test case insensitivity
        assert!(shell.matches_pattern(&PathBuf::from("/home/USER/project"), "user"));

        // Test non-matches
        assert!(!shell.matches_pattern(&PathBuf::from("/home/user/project"), "nonexistent"));
    }

    #[test]
    fn test_best_match() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test successful match
        let result = shell.best_match("rust").unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().to_string_lossy().contains("rust-app"));

        // Test no match
        let result = shell.best_match("nonexistent").unwrap();
        assert!(result.is_none());

        // Test empty pattern
        let result = shell.best_match("").unwrap();
        assert!(result.is_some()); // Should return most frecent project
    }

    #[test]
    fn test_all_projects() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();
        let results = shell.all_projects().unwrap();

        assert_eq!(results.len(), 7); // We added 7 test projects

        // Verify all paths are unique
        let mut paths = std::collections::HashSet::new();
        for path in &results {
            assert!(
                paths.insert(path.clone()),
                "Duplicate path found: {:?}",
                path
            );
        }
    }

    #[test]
    fn test_is_project_root() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test existing project
        let is_root = shell
            .is_project_root("/home/user/projects/rust-app")
            .unwrap();
        assert!(is_root);

        // Test non-existing path
        let is_root = shell.is_project_root("/nonexistent/path").unwrap();
        assert!(!is_root);

        // Test path that exists but isn't a project root
        let is_root = shell.is_project_root("/home/user").unwrap();
        assert!(!is_root);
    }

    #[test]
    fn test_record_access() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Record access to existing project
        let recorded = shell.record_access("/home/user/projects/node-web").unwrap();
        assert!(recorded);

        // Record access to non-project path
        let recorded = shell.record_access("/some/random/path").unwrap();
        assert!(!recorded);

        // Verify frecency was updated by checking if the project appears in frecent results
        let results = shell.query("", 10).unwrap();
        let node_web_found = results
            .iter()
            .any(|p| p.to_string_lossy().contains("node-web"));
        assert!(
            node_web_found,
            "Recently accessed project should appear in frecent results"
        );
    }

    #[test]
    fn test_query_frecency_ordering() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Get frecent results
        let results = shell.query("", 10).unwrap();

        // The rust-app should be first (accessed twice)
        if results.len() >= 2 {
            let first_path = results[0].to_string_lossy();
            assert!(
                first_path.contains("rust-app"),
                "Most frecent project should be first, got: {:?}",
                first_path
            );
        }
    }

    #[test]
    fn test_query_with_limit() {
        let (mut db, db_path) = create_test_db().unwrap();
        setup_test_data(&mut db).unwrap();

        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test with limit of 1
        let results = shell.query("project", 1).unwrap();
        assert_eq!(results.len(), 1);

        // Test with higher limit
        let results = shell.query("project", 5).unwrap();
        assert!(results.len() <= 5);
        assert!(results.len() > 0);
    }

    #[test]
    fn test_error_conditions() {
        let (_db, db_path) = create_test_db().unwrap();
        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test with invalid database path (should work since we create the DB)
        // The database creation handles invalid paths gracefully

        // Test query with very long pattern
        let long_pattern = "a".repeat(1000);
        let results = shell.query(&long_pattern, 10).unwrap();
        assert_eq!(results.len(), 0); // No matches expected

        // Test with special characters in pattern
        let results = shell.query("test@#$%", 10).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_path_edge_cases() {
        let (_db, db_path) = create_test_db().unwrap();
        let shell = ShellIntegration::new(&db_path).unwrap();

        // Test with root path
        let result = shell.matches_pattern(&PathBuf::from("/"), "root");
        assert!(!result);

        // Test with relative path
        let result = shell.matches_pattern(&PathBuf::from("relative/path"), "relative");
        assert!(result);

        // Test with path containing spaces
        let result = shell.matches_pattern(&PathBuf::from("/path with spaces/project"), "spaces");
        assert!(result);

        // Test with unicode characters
        let result = shell.matches_pattern(&PathBuf::from("/héllo/wörld"), "héllo");
        assert!(result);
    }
}
