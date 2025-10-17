//! Utility functions for the Durable Project Catalog
//!
//! This crate provides various utility functions used throughout the project,
//! including path manipulation, validation, formatting, and scanning helpers.

use dprojc_types::{ProjectIndicator, ScanConfig};
use regex::Regex;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};
use std::collections::HashSet;

/// Check if a directory should be excluded based on patterns
///
/// This function supports both exact matches and glob patterns with wildcards (* and ?).
/// Patterns are matched against directory names, and regex special characters are escaped.
///
/// # Examples
/// ```
/// use dprojc_utils::should_exclude_dir;
///
/// let patterns = vec!["node_modules".to_string(), "*.tmp".to_string()];
/// assert!(should_exclude_dir("node_modules", &patterns));
/// assert!(should_exclude_dir("cache.tmp", &patterns));
/// assert!(!should_exclude_dir("src", &patterns));
/// ```
pub fn should_exclude_dir(dir_name: &str, exclude_patterns: &[String]) -> bool {
    exclude_patterns.iter().any(|pattern| {
        // Handle exact matches first
        if dir_name == pattern {
            return true;
        }

        // Handle glob patterns
        if pattern.contains('*') || pattern.contains('?') {
            // Convert glob pattern to regex by escaping everything first, then converting wildcards
            let escaped = regex::escape(pattern);
            let regex_pattern = escaped
                .replace(r"\*", ".*")  // Convert escaped * back to .*
                .replace(r"\?", ".");  // Convert escaped ? to .

            if let Ok(regex) = Regex::new(&format!("^{}$", regex_pattern)) {
                regex.is_match(dir_name)
            } else {
                false
            }
        } else {
            false
        }
    })
}

/// Check if a path contains a project indicator
///
/// Scans the given path for files/directories that indicate the presence of a software project.
/// Returns a vector of all matching project indicators found.
///
/// # Examples
/// ```
/// use dprojc_utils::has_project_indicator;
/// use std::path::Path;
/// use std::fs;
/// use tempfile::tempdir;
///
/// let temp_dir = tempdir().unwrap();
/// fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
///
/// let indicators = vec!["package.json".to_string()];
/// let found = has_project_indicator(temp_dir.path(), &indicators);
/// assert!(!found.is_empty());
/// ```
pub fn has_project_indicator(path: &Path, indicators: &[String]) -> Vec<ProjectIndicator> {
    let mut found_indicators = Vec::new();

    for indicator in indicators {
        let indicator_path = path.join(indicator);
        if indicator_path.exists() {
            if let Some(indicator_type) = ProjectIndicator::from_path_name(indicator) {
                found_indicators.push(indicator_type);
            } else {
                found_indicators.push(ProjectIndicator::Custom(indicator.clone()));
            }
        }
    }

    found_indicators
}

/// Normalize a path to absolute path, resolving relative components
pub fn normalize_path(path: &Path) -> anyhow::Result<PathBuf> {
    // If path is already absolute, canonicalize it
    if path.is_absolute() {
        return path.canonicalize().map_err(Into::into);
    }

    // For relative paths, join with current directory and canonicalize
    let current_dir = get_current_dir()?;
    current_dir.join(path).canonicalize().map_err(Into::into)
}

/// Check if a directory entry should be skipped during walk
pub fn should_skip_entry(entry: &DirEntry, config: &ScanConfig) -> bool {
    let path = entry.path();

    // Skip if it's not a directory
    if !path.is_dir() {
        return true;
    }

    // Check max depth
    if let Some(max_depth) = config.max_depth {
        if entry.depth() > max_depth {
            return true;
        }
    }

    // Check exclude patterns
    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
        if should_exclude_dir(dir_name, &config.exclude_patterns) {
            return true;
        }
    }

    // Skip hidden directories unless they are project indicators
    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
        if dir_name.starts_with('.') && !config.project_indicators.contains(&dir_name.to_string()) {
            return true;
        }
    }

    false
}

/// Create a WalkDir iterator with proper configuration
pub fn create_walker(root: &Path, config: &ScanConfig) -> WalkDir {
    let mut walker = WalkDir::new(root).follow_links(config.follow_symlinks);

    if let Some(max_depth) = config.max_depth {
        walker = walker.max_depth(max_depth);
    }

    walker
}

/// Get the default database path
pub fn default_db_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let db_dir = home.join(".local").join("durable").join("durable-project-catalog");
    std::fs::create_dir_all(&db_dir)?;
    Ok(db_dir.join("catalog.db"))
}

/// Format a path for display, making it relative to home if possible
pub fn format_path_display(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            if relative.as_os_str().is_empty() {
                return "~/".to_string();
            } else {
                return format!("~/{}", relative.display());
            }
        }
    }
    path.display().to_string()
}

/// Validate that a path exists and is a directory
pub fn validate_scan_path(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Err(anyhow::anyhow!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(anyhow::anyhow!("Path is not a directory: {}", path.display()));
    }
    Ok(())
}

/// Check if a path is within another path (handles symlinks and relative paths)
///
/// Returns true if `target` is located within `base` or is the same path.
/// Both paths are normalized before comparison.
///
/// # Examples
/// ```
/// use dprojc_utils::is_path_within;
/// use std::path::Path;
///
/// let base = Path::new("/home/user");
/// let target = Path::new("/home/user/projects");
/// assert!(is_path_within(base, target));
/// assert!(is_path_within(base, base)); // Same path
/// ```
pub fn is_path_within(base: &Path, target: &Path) -> bool {
    let base = normalize_path(base).unwrap_or_else(|_| base.to_path_buf());
    let target = normalize_path(target).unwrap_or_else(|_| target.to_path_buf());

    target.starts_with(&base)
}

/// Get the relative path from a base path to a target path
pub fn get_relative_path(from: &Path, to: &Path) -> anyhow::Result<PathBuf> {
    let from = normalize_path(from)?;
    let to = normalize_path(to)?;

    to.strip_prefix(&from)
        .map(|p| p.to_path_buf())
        .map_err(|_| anyhow::anyhow!("Cannot make {} relative to {}", to.display(), from.display()))
}

/// Safely join multiple path components
pub fn safe_join_paths(base: &Path, components: &[&str]) -> PathBuf {
    let mut result = base.to_path_buf();
    for component in components {
        // Prevent directory traversal attacks
        if component.contains("..") || component.starts_with('/') || component.starts_with('\\') {
            continue;
        }
        result = result.join(component);
    }
    result
}

/// Check if a file has a specific extension (case-insensitive)
pub fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        extensions.iter().any(|&expected| ext.eq_ignore_ascii_case(expected))
    } else {
        false
    }
}

/// Get all project indicators as a HashSet for fast lookup
pub fn get_project_indicators_set() -> HashSet<String> {
    [
        ".git",
        "package.json",
        "Gemfile",
        ".gemspec",
        "Cargo.toml",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "devenv.nix",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Validate scan configuration for common issues
pub fn validate_scan_config(config: &ScanConfig) -> anyhow::Result<()> {
    if config.max_depth.is_some_and(|d| d == 0) {
        return Err(anyhow::anyhow!("max_depth cannot be 0"));
    }

    // Check for invalid exclude patterns
    for pattern in &config.exclude_patterns {
        if pattern.is_empty() {
            return Err(anyhow::anyhow!("Exclude pattern cannot be empty"));
        }
        if pattern.trim().is_empty() {
            return Err(anyhow::anyhow!("Exclude pattern cannot be whitespace-only"));
        }
        if pattern.contains('*') || pattern.contains('?') {
            // Test if the glob pattern can be converted to a valid regex
            let escaped = regex::escape(pattern);
            let regex_pattern = escaped
                .replace(r"\*", ".*")
                .replace(r"\?", ".");
            if Regex::new(&format!("^{}$", regex_pattern)).is_err() {
                return Err(anyhow::anyhow!("Invalid exclude pattern: {}", pattern));
            }
        }
    }

    Ok(())
}

/// Get a human-readable file size string
///
/// Formats byte counts into human-readable strings using appropriate units (B, KB, MB, GB, TB).
///
/// # Examples
/// ```
/// use dprojc_utils::format_file_size;
///
/// assert_eq!(format_file_size(0), "0 B");
/// assert_eq!(format_file_size(1024), "1.0 KB");
/// assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
/// ```
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Check if a path is a hidden file or directory (starts with '.')
pub fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Get the project type priority for sorting (higher numbers = higher priority)
pub fn get_project_type_priority(project_type: &dprojc_types::ProjectType) -> i32 {
    match project_type {
        dprojc_types::ProjectType::Rust => 10,
        dprojc_types::ProjectType::NodeJs => 9,
        dprojc_types::ProjectType::Python => 8,
        dprojc_types::ProjectType::Go => 7,
        dprojc_types::ProjectType::Java => 6,
        dprojc_types::ProjectType::Ruby => 5,
        dprojc_types::ProjectType::Nix => 4,
        dprojc_types::ProjectType::Git => 1,
        dprojc_types::ProjectType::Unknown => 0,
    }
}

/// Sanitize a string for use in file paths
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// Check if running on Windows
pub fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

/// Check if running on Unix-like system
pub fn is_unix() -> bool {
    cfg!(target_family = "unix")
}

/// Get the current working directory safely
pub fn get_current_dir() -> anyhow::Result<PathBuf> {
    std::env::current_dir().map_err(Into::into)
}

/// Expand tilde (~) in path strings
pub fn expand_tilde(path: &str) -> anyhow::Result<PathBuf> {
    if let Some(stripped) = path.strip_prefix('~') {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let relative_path = stripped.strip_prefix('/').unwrap_or(stripped);
        Ok(home.join(relative_path))
    } else {
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_should_exclude_dir() {
        let exclude_patterns = vec!["node_modules".to_string(), "*.tmp".to_string(), "test?".to_string()];
        assert!(should_exclude_dir("node_modules", &exclude_patterns));
        assert!(should_exclude_dir("test.tmp", &exclude_patterns));
        assert!(should_exclude_dir("test1", &exclude_patterns));
        assert!(!should_exclude_dir("src", &exclude_patterns));
        assert!(!should_exclude_dir("node_modules_backup", &exclude_patterns));
    }

    #[test]
    fn test_should_exclude_dir_complex_patterns() {
        let patterns = vec!["*.log".to_string(), "temp_*".to_string(), "cache".to_string()];
        assert!(should_exclude_dir("debug.log", &patterns));
        assert!(should_exclude_dir("temp_files", &patterns));
        assert!(should_exclude_dir("cache", &patterns));
        assert!(!should_exclude_dir("myapp.log.backup", &patterns));
    }

    #[test]
    fn test_has_project_indicator() {
        let temp_dir = tempdir().unwrap();
        let package_json = temp_dir.path().join("package.json");
        fs::write(&package_json, "{}").unwrap();

        let indicators = vec!["package.json".to_string()];
        let found = has_project_indicator(temp_dir.path(), &indicators);
        assert_eq!(found.len(), 1);
        assert!(matches!(found[0], ProjectIndicator::PackageJson));
    }

    #[test]
    fn test_has_project_indicator_multiple() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        let indicators = vec!["package.json".to_string(), "Cargo.toml".to_string()];
        let found = has_project_indicator(temp_dir.path(), &indicators);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&ProjectIndicator::PackageJson));
        assert!(found.contains(&ProjectIndicator::CargoToml));
    }

    #[test]
    fn test_has_project_indicator_custom() {
        let temp_dir = tempdir().unwrap();
        fs::write(temp_dir.path().join("custom_indicator"), "").unwrap();

        let indicators = vec!["custom_indicator".to_string()];
        let found = has_project_indicator(temp_dir.path(), &indicators);
        assert_eq!(found.len(), 1);
        match &found[0] {
            ProjectIndicator::Custom(s) => assert_eq!(s, "custom_indicator"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_normalize_path() {
        let temp_dir = tempdir().unwrap();
        let absolute_path = normalize_path(temp_dir.path()).unwrap();
        assert!(absolute_path.is_absolute());

        // Test with relative path
        let relative_path = Path::new("src/lib.rs");
        let normalized = normalize_path(relative_path).unwrap();
        assert!(normalized.is_absolute());
    }

    #[test]
    fn test_normalize_path_errors() {
        let non_existent = Path::new("/non/existent/path/that/does/not/exist");
        assert!(normalize_path(non_existent).is_err());
    }

    #[test]
    fn test_validate_scan_path() {
        let temp_dir = tempdir().unwrap();
        assert!(validate_scan_path(temp_dir.path()).is_ok());

        let non_existent = temp_dir.path().join("nonexistent");
        assert!(validate_scan_path(&non_existent).is_err());

        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "test").unwrap();
        assert!(validate_scan_path(&file_path).is_err());
    }

    #[test]
    fn test_is_path_within() {
        let temp_dir = tempdir().unwrap();
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        assert!(is_path_within(temp_dir.path(), &sub_dir));
        assert!(!is_path_within(&sub_dir, temp_dir.path()));
        assert!(is_path_within(temp_dir.path(), temp_dir.path()));
    }

    #[test]
    fn test_get_relative_path() {
        let temp_dir = tempdir().unwrap();
        let base = temp_dir.path();
        let target = base.join("projects").join("rust");
        fs::create_dir_all(&target).unwrap();

        let relative = get_relative_path(base, &target).unwrap();
        assert_eq!(relative, Path::new("projects/rust"));
    }

    #[test]
    fn test_get_relative_path_error() {
        let base = Path::new("/home/user/projects");
        let target = Path::new("/home/other");
        assert!(get_relative_path(base, target).is_err());
    }

    #[test]
    fn test_safe_join_paths() {
        let base = Path::new("/home/user");
        let components = vec!["projects", "rust", "src"];
        let result = safe_join_paths(base, &components);
        assert_eq!(result, Path::new("/home/user/projects/rust/src"));
    }

    #[test]
    fn test_safe_join_paths_prevents_traversal() {
        let base = Path::new("/home/user");
        let components = vec!["..", "etc", "passwd"];
        let result = safe_join_paths(base, &components);
        // Should skip the .. component
        assert_eq!(result, Path::new("/home/user/etc/passwd"));
    }

    #[test]
    fn test_has_extension() {
        assert!(has_extension(Path::new("file.rs"), &["rs"]));
        assert!(has_extension(Path::new("file.RS"), &["rs"]));
        assert!(!has_extension(Path::new("file.rs"), &["js"]));
        assert!(!has_extension(Path::new("file"), &["rs"]));
    }

    #[test]
    fn test_get_project_indicators_set() {
        let indicators = get_project_indicators_set();
        assert!(indicators.contains(".git"));
        assert!(indicators.contains("package.json"));
        assert!(indicators.contains("Cargo.toml"));
        assert_eq!(indicators.len(), 9);
    }

    #[test]
    fn test_validate_scan_config() {
        let valid_config = ScanConfig::default();
        assert!(validate_scan_config(&valid_config).is_ok());

        let invalid_config = ScanConfig {
            max_depth: Some(0),
            ..ScanConfig::default()
        };
        assert!(validate_scan_config(&invalid_config).is_err());
    }

    #[test]
    fn test_validate_scan_config_invalid_pattern() {
        // Test empty pattern
        let config_empty = ScanConfig {
            exclude_patterns: vec!["".to_string()],
            ..ScanConfig::default()
        };
        assert!(validate_scan_config(&config_empty).is_err());

        // Test whitespace-only pattern
        let config_whitespace = ScanConfig {
            exclude_patterns: vec!["   ".to_string()],
            ..ScanConfig::default()
        };
        assert!(validate_scan_config(&config_whitespace).is_err());

        // Test valid glob patterns
        let config_valid = ScanConfig {
            exclude_patterns: vec!["test*".to_string(), "valid?".to_string()],
            ..ScanConfig::default()
        };
        assert!(validate_scan_config(&config_valid).is_ok());
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(1023), "1023 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_is_hidden_path() {
        assert!(is_hidden_path(Path::new(".hidden")));
        assert!(is_hidden_path(Path::new(".git")));
        assert!(!is_hidden_path(Path::new("normal")));
        assert!(!is_hidden_path(Path::new("file.txt")));
    }

    #[test]
    fn test_get_project_type_priority() {
        assert_eq!(get_project_type_priority(&dprojc_types::ProjectType::Rust), 10);
        assert_eq!(get_project_type_priority(&dprojc_types::ProjectType::NodeJs), 9);
        assert_eq!(get_project_type_priority(&dprojc_types::ProjectType::Unknown), 0);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_file"), "normal_file");
        assert_eq!(sanitize_filename("file:with*chars?"), "file_with_chars_");
        assert_eq!(sanitize_filename("path/to/file"), "path_to_file");
    }

    #[test]
    fn test_expand_tilde() {
        // Test tilde expansion
        let test_path = "~/test";
        assert!(test_path.starts_with('~'), "Test path should start with tilde");
        let result = expand_tilde(test_path);
        assert!(result.is_ok());
        let expanded = result.unwrap();
        if let Some(home) = dirs::home_dir() {
            let expected = home.join("test");
            assert_eq!(expanded, expected, "Tilde should be expanded to home directory. Got: {:?}, Expected: {:?}", expanded, expected);
        } else {
            // If no home directory, should return the original path
            assert_eq!(expanded, PathBuf::from("~/test"));
        }

        // Test no tilde
        let no_tilde = expand_tilde("/absolute/path").unwrap();
        assert_eq!(no_tilde, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_format_path_display() {
        if let Some(home) = dirs::home_dir() {
            let home_subpath = home.join("projects");
            let display = format_path_display(&home_subpath);
            assert!(display.starts_with("~/"));
        }

        let absolute_path = Path::new("/tmp/test");
        let display = format_path_display(absolute_path);
        assert_eq!(display, "/tmp/test");
    }

    #[test]
    fn test_format_path_display_home() {
        if let Some(home) = dirs::home_dir() {
            let display = format_path_display(&home);
            assert_eq!(display, "~/");
        }
    }

    #[test]
    fn test_should_skip_entry() {
        let temp_dir = tempdir().unwrap();
        let config = ScanConfig::default();

        // Test with a subdirectory (not root)
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        let walker = walkdir::WalkDir::new(temp_dir.path());
        for entry in walker {
            let entry = entry.unwrap();
            if entry.path() == sub_dir {
                assert!(!should_skip_entry(&entry, &config));
                break;
            }
        }

        // Test with excluded directory
        let excluded_dir = temp_dir.path().join("node_modules");
        fs::create_dir(&excluded_dir).unwrap();
        let walker = walkdir::WalkDir::new(temp_dir.path());
        for entry in walker {
            let entry = entry.unwrap();
            if entry.path() == excluded_dir {
                assert!(should_skip_entry(&entry, &config));
                break;
            }
        }
    }

    #[test]
    fn test_should_skip_entry_max_depth() {
        let temp_dir = tempdir().unwrap();
        let deep_dir = temp_dir.path().join("level1").join("level2");
        fs::create_dir_all(&deep_dir).unwrap();

        let mut config = ScanConfig::default();
        config.max_depth = Some(1);

        // Walk from temp_dir, so level2 should be at depth 2
        let walker = walkdir::WalkDir::new(temp_dir.path());
        for entry in walker {
            let entry = entry.unwrap();
            if entry.path() == deep_dir {
                assert_eq!(entry.depth(), 2);
                assert!(should_skip_entry(&entry, &config));
                break;
            }
        }
    }

    #[test]
    fn test_create_walker() {
        let temp_dir = tempdir().unwrap();
        let config = ScanConfig::default();
        let walker = create_walker(temp_dir.path(), &config);
        // Just test that it creates a walker without panicking
        let _count = walker.into_iter().count();
        assert!(true);
    }

    #[test]
    fn test_default_db_path() {
        let db_path = default_db_path().unwrap();
        assert!(db_path.ends_with("catalog.db"));
        assert!(db_path.parent().unwrap().ends_with(".local/durable/durable-project-catalog"));
    }

    #[test]
    fn test_platform_functions() {
        // These tests depend on the actual platform
        #[cfg(target_os = "linux")]
        assert!(is_unix());

        #[cfg(target_os = "windows")]
        assert!(is_windows());
    }

    #[test]
    fn test_get_current_dir() {
        let current = get_current_dir().unwrap();
        assert!(current.is_absolute());
        assert!(current.is_dir());
    }
}