use dprojc_types::{Project, ProjectType, ScanConfig, ScanError, ScanErrorType, ScanResult};
use dprojc_utils::{create_walker, has_project_indicator, should_exclude_dir, should_skip_entry, validate_scan_config, validate_scan_path};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Check if a project path should be skipped based on exclude patterns
fn should_skip_project(path: &Path, exclude_patterns: &[String]) -> bool {
    path.components().any(|comp| {
        if let std::path::Component::Normal(name) = comp {
            if let Some(name_str) = name.to_str() {
                should_exclude_dir(name_str, exclude_patterns)
            } else {
                false
            }
        } else {
            false
        }
    })
}

/// The main scanner struct
pub struct ProjectScanner {
    config: ScanConfig,
}

impl ProjectScanner {
    /// Create a new scanner with default configuration
    pub fn new() -> anyhow::Result<Self> {
        let config = ScanConfig::default();
        validate_scan_config(&config)?;
        Ok(Self { config })
    }

    /// Create a new scanner with custom configuration
    pub fn with_config(config: ScanConfig) -> anyhow::Result<Self> {
        validate_scan_config(&config)?;
        Ok(Self { config })
    }

    /// Scan a directory for projects
    pub async fn scan(&self, root_path: &Path) -> anyhow::Result<ScanResult> {
        validate_scan_path(root_path)?;

        // Convert root_path to absolute path
        let root_path_abs = if root_path.is_absolute() {
            root_path.to_path_buf()
        } else {
            std::env::current_dir()?.join(root_path)
        };

        let start_time = std::time::Instant::now();
        let mut projects = Vec::new();
        let mut excluded_dirs = Vec::new();
        let mut errors = Vec::new();
        let mut dirs_scanned = 0;

        // Check the root directory for project indicators
        let root_indicators = has_project_indicator(&root_path_abs, &self.config.project_indicators);
        if !root_indicators.is_empty() && !should_skip_project(&root_path_abs, &self.config.exclude_patterns) {
            let project = Project {
                path: root_path_abs.clone(),
                project_type: ProjectType::from_indicators(&root_indicators),
                indicators: root_indicators,
                last_scanned: chrono::Utc::now(),
            };
            projects.push(project);
        }

        let walker = create_walker(&root_path_abs, &self.config);

        for entry in walker {
            match entry {
                Ok(entry) => {
                    dirs_scanned += 1;

                    if entry.depth() == 0 || should_skip_entry(&entry, &self.config) {
                        if entry.depth() != 0 && entry.path().is_dir() {
                            excluded_dirs.push(entry.path().to_path_buf());
                        }
                        continue;
                    }

                    // Check for project indicators
                    let indicators = has_project_indicator(entry.path(), &self.config.project_indicators);
                    if !indicators.is_empty() && !should_skip_project(entry.path(), &self.config.exclude_patterns) {
                        // entry.path() from walkdir is already absolute since we use root_path_abs
                        let project = Project {
                            path: entry.path().to_path_buf(),
                            project_type: ProjectType::from_indicators(&indicators),
                            indicators,
                            last_scanned: chrono::Utc::now(),
                        };
                        projects.push(project);
                    }
                }
                Err(err) => {
                    let error = ScanError {
                        path: err.path().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("unknown")),
                        error_type: ScanErrorType::IoError,
                        message: err.to_string(),
                    };
                    errors.push(error);
                }
            }
        }

        let scan_duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(ScanResult {
            root_path: root_path_abs,
            projects,
            excluded_dirs,
            errors,
            dirs_scanned,
            scan_duration_ms,
        })
    }

    /// Scan multiple directories concurrently
    pub async fn scan_multiple(&self, paths: &[PathBuf]) -> anyhow::Result<Vec<ScanResult>> {
        let mut handles = Vec::new();

        for path in paths {
            let path = path.clone();
            let config = self.config.clone();
            let handle = tokio::spawn(async move {
                let scanner = ProjectScanner::with_config(config)?;
                scanner.scan(&path).await
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result?),
                Err(err) => return Err(anyhow::anyhow!("Task join error: {}", err)),
            }
        }

        Ok(results)
    }

    /// Get the current configuration
    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: ScanConfig) {
        self.config = config;
    }
}

impl Default for ProjectScanner {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

/// Async scanner that can be shared across threads
#[derive(Clone)]
pub struct SharedScanner(Arc<Mutex<ProjectScanner>>);

impl SharedScanner {
    /// Create a new shared scanner
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(ProjectScanner::new().unwrap())))
    }

    /// Create a new shared scanner with config
    pub fn with_config(config: ScanConfig) -> Self {
        Self(Arc::new(Mutex::new(ProjectScanner::with_config(config).unwrap())))
    }
}

impl Default for SharedScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedScanner {
    /// Scan a directory for projects
    pub async fn scan(&self, root_path: &Path) -> anyhow::Result<ScanResult> {
        let scanner = self.0.lock().await;
        scanner.scan(root_path).await
    }

    /// Scan multiple directories concurrently
    pub async fn scan_multiple(&self, paths: &[PathBuf]) -> anyhow::Result<Vec<ScanResult>> {
        let scanner = self.0.lock().await;
        scanner.scan_multiple(paths).await
    }

    /// Get the current configuration
    pub async fn config(&self) -> ScanConfig {
        let scanner = self.0.lock().await;
        scanner.config().clone()
    }

    /// Update the configuration
    pub async fn set_config(&self, config: ScanConfig) {
        let mut scanner = self.0.lock().await;
        scanner.set_config(config);
    }
}

/// Utility function to scan a single directory
pub async fn scan_directory(path: &Path) -> anyhow::Result<ScanResult> {
    let scanner = ProjectScanner::new()?;
    scanner.scan(path).await
}

/// Utility function to scan with custom config
pub async fn scan_directory_with_config(path: &Path, config: ScanConfig) -> anyhow::Result<ScanResult> {
    let scanner = ProjectScanner::with_config(config)?;
    scanner.scan(path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use dprojc_types::ProjectIndicator;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let temp_dir = tempdir().unwrap();
        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 0);
        assert!(result.errors.is_empty());
        assert!(result.dirs_scanned > 0);
    }

    #[tokio::test]
    async fn test_scan_directory_with_git() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Git);
        assert!(result.projects[0].indicators.contains(&ProjectIndicator::GitDirectory));
    }

    #[tokio::test]
    async fn test_scan_directory_with_package_json() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::NodeJs);
        assert!(result.projects[0].indicators.contains(&ProjectIndicator::PackageJson));
    }

    #[tokio::test]
    async fn test_scan_excludes_node_modules() {
        let temp_dir = tempdir().unwrap();
        let node_modules = temp_dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::create_dir(node_modules.join("some-package")).unwrap();
        fs::write(node_modules.join("some-package").join("package.json"), "{}").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should not find the project in node_modules
        assert_eq!(result.projects.len(), 0);
        assert!(result.excluded_dirs.iter().any(|p| p.ends_with("node_modules")));
    }

    #[tokio::test]
    async fn test_scan_multiple_directories() {
        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();

        fs::create_dir(temp_dir1.path().join(".git")).unwrap();
        fs::write(temp_dir2.path().join("Cargo.toml"), "[package]").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let paths = vec![temp_dir1.path().to_path_buf(), temp_dir2.path().to_path_buf()];
        let results = scanner.scan_multiple(&paths).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].projects.len(), 1);
        assert_eq!(results[1].projects.len(), 1);
        assert_eq!(results[0].projects[0].project_type, ProjectType::Git);
        assert_eq!(results[1].projects[0].project_type, ProjectType::Rust);
    }

    #[tokio::test]
    async fn test_scan_invalid_path() {
        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(Path::new("/nonexistent/path")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_custom_config() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join("custom_indicator")).unwrap();

        let mut config = ScanConfig::default();
        config.project_indicators.push("custom_indicator".to_string());

        let scanner = ProjectScanner::with_config(config).unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert!(result.projects[0].indicators.contains(&ProjectIndicator::Custom("custom_indicator".to_string())));
    }

    #[tokio::test]
    async fn test_scan_directory_with_multiple_indicators() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::NodeJs); // PackageJson takes precedence over Git
        assert!(result.projects[0].indicators.contains(&ProjectIndicator::GitDirectory));
        assert!(result.projects[0].indicators.contains(&ProjectIndicator::PackageJson));
    }

    #[tokio::test]
    async fn test_scan_nested_projects() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        let sub_dir = temp_dir.path().join("subproject");
        fs::create_dir(&sub_dir).unwrap();
        fs::write(sub_dir.join("Cargo.toml"), "[package]").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 2);
        let git_project = result.projects.iter().find(|p| p.project_type == ProjectType::Git).unwrap();
        let rust_project = result.projects.iter().find(|p| p.project_type == ProjectType::Rust).unwrap();
        assert_eq!(git_project.path, temp_dir.path());
        assert_eq!(rust_project.path, sub_dir);
    }

    #[tokio::test]
    async fn test_scan_with_max_depth() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        let deep_dir = temp_dir.path().join("level1").join("level2");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(deep_dir.join("Cargo.toml"), "[package]").unwrap();

        let mut config = ScanConfig::default();
        config.max_depth = Some(1);

        let scanner = ProjectScanner::with_config(config).unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Git);
    }

    #[tokio::test]
    async fn test_scan_excludes_hidden_dirs() {
        let temp_dir = tempdir().unwrap();
        let hidden_dir = temp_dir.path().join(".hidden_project");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("package.json"), "{}").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 0);
        assert!(result.excluded_dirs.iter().any(|p| p.ends_with(".hidden_project")));
    }

    #[tokio::test]
    async fn test_shared_scanner() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let scanner = SharedScanner::new();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Git);
    }

    #[tokio::test]
    async fn test_scan_multiple_with_errors() {
        let temp_dir1 = tempdir().unwrap();
        fs::create_dir(temp_dir1.path().join(".git")).unwrap();

        let invalid_path = Path::new("/nonexistent/path");

        let scanner = ProjectScanner::new().unwrap();
        let results = scanner.scan_multiple(&[temp_dir1.path().to_path_buf(), invalid_path.to_path_buf()]).await;

        assert!(results.is_err()); // Should fail due to invalid path
    }

    #[tokio::test]
    async fn test_utility_functions() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let result = scan_directory(temp_dir.path()).await.unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::NodeJs);

        let config = ScanConfig::default();
        let result2 = scan_directory_with_config(temp_dir.path(), config).await.unwrap();
        assert_eq!(result2.projects.len(), 1);
    }

    #[test]
    fn test_invalid_config_validation() {
        let invalid_config = ScanConfig {
            max_depth: Some(0),
            ..ScanConfig::default()
        };
        assert!(ProjectScanner::with_config(invalid_config).is_err());
    }

    #[tokio::test]
    async fn test_project_types() {
        let test_cases = vec![
            ("Gemfile", ProjectType::Ruby),
            ("pyproject.toml", ProjectType::Python),
            ("go.mod", ProjectType::Go),
            ("pom.xml", ProjectType::Java),
            ("devenv.nix", ProjectType::Nix),
        ];

        for (indicator, expected_type) in test_cases {
            let temp_dir = tempdir().unwrap();
            fs::write(temp_dir.path().join(indicator), "").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

            assert_eq!(result.projects.len(), 1, "Failed for {}", indicator);
            assert_eq!(result.projects[0].project_type, expected_type, "Failed for {}", indicator);
        }
    }

    #[tokio::test]
    async fn test_shared_scanner_concurrent_access() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let scanner = SharedScanner::new();
        let path = temp_dir.path().to_path_buf();

        // Test concurrent access
        let handles: Vec<_> = (0..5).map(|_| {
            let scanner = scanner.clone();
            let path = path.clone();
            tokio::spawn(async move {
                scanner.scan(&path).await
            })
        }).collect();

        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            assert_eq!(result.projects.len(), 1);
            assert_eq!(result.projects[0].project_type, ProjectType::Git);
        }
    }

    #[tokio::test]
    async fn test_scan_with_symlinks() {
        let temp_dir = tempdir().unwrap();
        let real_project = temp_dir.path().join("real_project");
        fs::create_dir(&real_project).unwrap();
        fs::write(real_project.join("Cargo.toml"), "[package]").unwrap();

        // Create a symlink to the project
        let link_path = temp_dir.path().join("project_link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_project, &link_path).unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should find the real project. The symlink behavior depends on walker configuration
        // but we expect at least the real project to be found
        assert!(result.projects.len() >= 1);
        assert!(result.projects.iter().any(|p| p.path == real_project));
    }

    #[tokio::test]
    async fn test_scan_with_permission_denied() {
        let temp_dir = tempdir().unwrap();
        let restricted_dir = temp_dir.path().join("restricted");
        fs::create_dir(&restricted_dir).unwrap();

        // Make directory inaccessible (on Unix systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&restricted_dir, perms).unwrap();
        }

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should have errors for the inaccessible directory
        #[cfg(unix)]
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_scan_performance_metrics() {
        let temp_dir = tempdir().unwrap();

        // Create a moderately complex directory structure
        for i in 0..10 {
            let sub_dir = temp_dir.path().join(format!("project_{}", i));
            fs::create_dir(&sub_dir).unwrap();
            fs::write(sub_dir.join("package.json"), "{}").unwrap();

            // Add some nested structure
            let nested = sub_dir.join("nested");
            fs::create_dir(&nested).unwrap();
            fs::create_dir(nested.join("node_modules")).unwrap(); // Should be excluded
        }

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should find 10 projects
        assert_eq!(result.projects.len(), 10);
        // Should have scanned many directories
        assert!(result.dirs_scanned > 10);
        // Should have some excluded directories (node_modules)
        assert!(!result.excluded_dirs.is_empty());
        // Should have reasonable scan duration (allow 0 for very fast systems)
        assert!(result.scan_duration_ms < 1000); // Should be fast
    }

    #[tokio::test]
    async fn test_config_update_during_scan() {
        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let scanner = SharedScanner::new();

        // Start a scan
        let path = temp_dir.path().to_path_buf();
        let scan_handle = {
            let scanner = scanner.clone();
            let path = path.clone();
            tokio::spawn(async move {
                scanner.scan(&path).await
            })
        };

        // Update config while scan is potentially running
        let mut new_config = ScanConfig::default();
        new_config.max_depth = Some(1);
        scanner.set_config(new_config).await;

        // Wait for scan to complete
        let result = scan_handle.await.unwrap().unwrap();
        assert_eq!(result.projects.len(), 1);

        // Verify config was updated
        let current_config = scanner.config().await;
        assert_eq!(current_config.max_depth, Some(1));
    }

    #[tokio::test]
    async fn test_scan_with_custom_exclude_patterns() {
        let temp_dir = tempdir().unwrap();

        // Create a project in a directory that should be excluded
        let custom_excluded = temp_dir.path().join("custom_exclude");
        fs::create_dir(&custom_excluded).unwrap();
        fs::write(custom_excluded.join("Cargo.toml"), "[package]").unwrap();

        // Create a normal project
        let normal_project = temp_dir.path().join("normal");
        fs::create_dir(&normal_project).unwrap();
        fs::write(normal_project.join("package.json"), "{}").unwrap();

        let mut config = ScanConfig::default();
        config.exclude_patterns.push("custom_exclude".to_string());

        let scanner = ProjectScanner::with_config(config).unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should only find the normal project
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::NodeJs);
        assert!(result.excluded_dirs.iter().any(|p| p.ends_with("custom_exclude")));
    }

    #[tokio::test]
    async fn test_scan_root_with_multiple_indicators_precedence() {
        let temp_dir = tempdir().unwrap();

        // Add multiple indicators to root directory
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should find one project with correct precedence (Rust > Node.js > Git)
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
        assert_eq!(result.projects[0].indicators.len(), 3); // All three indicators
    }

    #[tokio::test]
    async fn test_scan_with_very_deep_structure() {
        let temp_dir = tempdir().unwrap();

        // Create a deep directory structure within default max_depth (10)
        let mut current_path = temp_dir.path().to_path_buf();
        for i in 0..8 {  // Stay within default max_depth of 10
            current_path = current_path.join(format!("level_{}", i));
            fs::create_dir(&current_path).unwrap();
        }
        fs::write(current_path.join("Cargo.toml"), "[package]").unwrap();

        let scanner = ProjectScanner::new().unwrap();
        let result = scanner.scan(temp_dir.path()).await.unwrap();

        // Should find the project at the deep level
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].project_type, ProjectType::Rust);
        assert!(result.dirs_scanned > 8);
    }

    #[tokio::test]
    async fn test_shared_scanner_config_methods() {
        let scanner = SharedScanner::new();

        // Test getting default config
        let config = scanner.config().await;
        assert_eq!(config.max_depth, Some(10)); // Default max_depth

        // Test setting config
        let mut new_config = ScanConfig::default();
        new_config.max_depth = Some(5);
        scanner.set_config(new_config).await;

        // Test getting updated config
        let updated_config = scanner.config().await;
        assert_eq!(updated_config.max_depth, Some(5));
    }
}
