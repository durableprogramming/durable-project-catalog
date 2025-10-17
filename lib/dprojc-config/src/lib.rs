//! Configuration management for the Durable Project Catalog
//!
//! This crate provides functionality to load and manage configuration for the
//! Durable Project Catalog scanner. Configuration can be loaded from:
//!
//! 1. YAML configuration files
//! 2. Environment variables
//! 3. Default values
//!
//! Configuration sources are merged in order of precedence: environment variables
//! override configuration files, which override defaults.
//!
//! # Configuration File Format
//!
//! Configuration files use YAML format:
//!
//! ```yaml
//! max_depth: 10
//! exclude_patterns:
//!   - node_modules
//!   - vendor
//! project_indicators:
//!   - .git
//!   - package.json
//! follow_symlinks: false
//! ```
//!
//! # Environment Variables
//!
//! - `DURABLE_MAX_DEPTH`: Maximum scan depth (integer)
//! - `DURABLE_EXCLUDE_PATTERNS`: Comma-separated list of patterns to exclude
//! - `DURABLE_PROJECT_INDICATORS`: Comma-separated list of project indicators
//! - `DURABLE_FOLLOW_SYMLINKS`: Whether to follow symlinks (true/false)
//!
//! # Configuration File Locations
//!
//! Configuration files are searched in the following locations (in order):
//!
//! 1. `./config.yaml`
//! 2. `./.durable.yaml`
//! 3. `~/.config/durable/config.yaml`
//! 4. `~/.config/durable/.durable.yaml`
//! 5. `~/.durable.yaml`

use dprojc_types::ScanConfig;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

/// Configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Maximum depth to scan
    pub max_depth: Option<usize>,
    /// Patterns to exclude from scanning
    pub exclude_patterns: Option<Vec<String>>,
    /// Additional project indicators to check
    pub project_indicators: Option<Vec<String>>,
    /// Whether to follow symbolic links
    pub follow_symlinks: Option<bool>,
}

/// Configuration manager for loading and merging configurations
pub struct ConfigManager;

impl ConfigManager {
    /// Load configuration from default sources in order of precedence:
    /// 1. Environment variables
    /// 2. Configuration file (searched in standard locations)
    /// 3. Default values
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use dprojc_config::ConfigManager;
    ///
    /// let config = ConfigManager::load_config().unwrap();
    /// println!("Max depth: {:?}", config.max_depth);
    /// ```
    pub fn load_config() -> anyhow::Result<ScanConfig> {
        let mut config = ScanConfig::default();

        // Load from config file
        if let Some(file_config) = Self::load_from_file()? {
            Self::merge_config_file(&mut config, file_config);
        }

        // Load from environment variables (overrides file config)
        Self::load_from_env(&mut config)?;

        Self::validate_config(&config)?;

        Ok(config)
    }

    /// Load configuration from a specific file path
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use dprojc_config::ConfigManager;
    /// use std::path::Path;
    ///
    /// let config = ConfigManager::load_from_path("my-config.yaml").unwrap();
    /// ```
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<ScanConfig> {
        let mut config = ScanConfig::default();

        if let Some(file_config) = Self::load_config_file(path)? {
            Self::merge_config_file(&mut config, file_config);
        }

        // Still apply environment overrides
        Self::load_from_env(&mut config)?;

        Self::validate_config(&config)?;

        Ok(config)
    }

    /// Load configuration from a specific file path without environment overrides
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use dprojc_config::ConfigManager;
    /// use std::path::Path;
    ///
    /// let config = ConfigManager::load_from_file_only("my-config.yaml").unwrap();
    /// ```
    pub fn load_from_file_only<P: AsRef<Path>>(path: P) -> anyhow::Result<ScanConfig> {
        let mut config = ScanConfig::default();

        if let Some(file_config) = Self::load_config_file(path)? {
            Self::merge_config_file(&mut config, file_config);
        }

        Self::validate_config(&config)?;

        Ok(config)
    }

    /// Load configuration from environment variables only
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use dprojc_config::ConfigManager;
    ///
    /// // Set environment variables first
    /// std::env::set_var("DURABLE_MAX_DEPTH", "15");
    ///
    /// let config = ConfigManager::load_from_env_only().unwrap();
    /// assert_eq!(config.max_depth, Some(15));
    /// ```
    pub fn load_from_env_only() -> anyhow::Result<ScanConfig> {
        let mut config = ScanConfig::default();
        Self::load_from_env(&mut config)?;
        Self::validate_config(&config)?;
        Ok(config)
    }

    /// Get the default configuration file paths to check
    pub fn get_config_paths() -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        // Current directory
        paths.push(std::path::PathBuf::from("config.yaml"));
        paths.push(std::path::PathBuf::from(".durable.yaml"));

        // User config directory
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("durable").join("config.yaml"));
            paths.push(config_dir.join("durable").join(".durable.yaml"));
        }

        // Home directory
        if let Some(home_dir) = dirs::home_dir() {
            paths.push(home_dir.join(".durable.yaml"));
        }

        paths
    }

    fn load_from_file() -> anyhow::Result<Option<ConfigFile>> {
        for path in Self::get_config_paths() {
            if path.exists() {
                return Self::load_config_file(&path);
            }
        }
        Ok(None)
    }

    fn load_config_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Option<ConfigFile>> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;

        let config: ConfigFile = serde_yaml::from_str(&contents).map_err(|e| {
            anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e)
        })?;

        Ok(Some(config))
    }

    fn merge_config_file(config: &mut ScanConfig, file_config: ConfigFile) {
        if let Some(max_depth) = file_config.max_depth {
            config.max_depth = Some(max_depth);
        }
        if let Some(exclude_patterns) = file_config.exclude_patterns {
            config.exclude_patterns = exclude_patterns
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
        }
        if let Some(project_indicators) = file_config.project_indicators {
            config.project_indicators = project_indicators
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect();
        }
        if let Some(follow_symlinks) = file_config.follow_symlinks {
            config.follow_symlinks = follow_symlinks;
        }
    }

    fn load_from_env(config: &mut ScanConfig) -> anyhow::Result<()> {
        if let Ok(max_depth_str) = env::var("DURABLE_MAX_DEPTH") {
            if let Ok(max_depth) = max_depth_str.trim().parse::<usize>() {
                config.max_depth = Some(max_depth);
            }
        }

        if let Ok(exclude_patterns_str) = env::var("DURABLE_EXCLUDE_PATTERNS") {
            let trimmed = exclude_patterns_str.trim();
            if !trimmed.is_empty() {
                let patterns = trimmed
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if !patterns.is_empty() {
                    config.exclude_patterns = patterns;
                }
            }
        }

        if let Ok(project_indicators_str) = env::var("DURABLE_PROJECT_INDICATORS") {
            let trimmed = project_indicators_str.trim();
            if !trimmed.is_empty() {
                let indicators = trimmed
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if !indicators.is_empty() {
                    config.project_indicators = indicators;
                }
            }
        }

        if let Ok(follow_symlinks_str) = env::var("DURABLE_FOLLOW_SYMLINKS") {
            if let Ok(follow_symlinks) = follow_symlinks_str.trim().parse::<bool>() {
                config.follow_symlinks = follow_symlinks;
            }
        }

        Ok(())
    }

    /// Validate configuration values
    pub fn validate_config(config: &ScanConfig) -> anyhow::Result<()> {
        if let Some(max_depth) = config.max_depth {
            if max_depth == 0 {
                return Err(anyhow::anyhow!("max_depth must be greater than 0"));
            }
            if max_depth > 1000 {
                return Err(anyhow::anyhow!(
                    "max_depth must be less than or equal to 1000"
                ));
            }
        }
        for pattern in &config.exclude_patterns {
            if pattern.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "exclude_patterns cannot contain empty or whitespace-only strings"
                ));
            }
        }
        for indicator in &config.project_indicators {
            if indicator.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "project_indicators cannot contain empty or whitespace-only strings"
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = ScanConfig::default();
        assert_eq!(config.max_depth, Some(10));
        assert!(config
            .exclude_patterns
            .contains(&"node_modules".to_string()));
        assert!(config.project_indicators.contains(&".git".to_string()));
        assert!(!config.follow_symlinks);
    }

    #[test]
    fn test_load_from_env_only() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();

        // Clear all env vars first
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");

        // Set env to defaults
        env::set_var("DURABLE_MAX_DEPTH", "10");
        env::set_var("DURABLE_FOLLOW_SYMLINKS", "false");

        let config = ConfigManager::load_from_env_only().unwrap();

        // Should use defaults for unset values
        assert_eq!(config.max_depth, Some(10));
        assert!(config
            .exclude_patterns
            .contains(&"node_modules".to_string()));
        assert!(!config.follow_symlinks);

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
    }

    #[test]
    fn test_load_config_file() {
        let yaml_content = r#"
max_depth: 15
exclude_patterns:
  - test_exclude
project_indicators:
  - test_indicator
follow_symlinks: true
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", yaml_content).unwrap();

        let file_config = ConfigManager::load_config_file(temp_file.path())
            .unwrap()
            .unwrap();

        assert_eq!(file_config.max_depth, Some(15));
        assert_eq!(
            file_config.exclude_patterns,
            Some(vec!["test_exclude".to_string()])
        );
        assert_eq!(
            file_config.project_indicators,
            Some(vec!["test_indicator".to_string()])
        );
        assert_eq!(file_config.follow_symlinks, Some(true));
    }

    #[test]
    fn test_merge_config_file() {
        let mut config = ScanConfig::default();
        let file_config = ConfigFile {
            max_depth: Some(20),
            exclude_patterns: Some(vec!["merged_exclude".to_string()]),
            project_indicators: Some(vec!["merged_indicator".to_string()]),
            follow_symlinks: Some(true),
        };

        ConfigManager::merge_config_file(&mut config, file_config);

        assert_eq!(config.max_depth, Some(20));
        assert_eq!(config.exclude_patterns, vec!["merged_exclude"]);
        assert_eq!(config.project_indicators, vec!["merged_indicator"]);
        assert!(config.follow_symlinks);
    }

    #[test]
    fn test_get_config_paths() {
        let paths = ConfigManager::get_config_paths();
        assert!(!paths.is_empty());
        // Should include current directory config.yaml
        assert!(paths.contains(&std::path::PathBuf::from("config.yaml")));
    }

    #[test]
    fn test_load_from_path() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();

        // Clear any existing env vars
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");

        let yaml_content = r#"
max_depth: 25
exclude_patterns:
  - path_exclude
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", yaml_content).unwrap();

        let config = ConfigManager::load_from_path(temp_file.path()).unwrap();

        assert_eq!(config.max_depth, Some(25));
        assert_eq!(config.exclude_patterns, vec!["path_exclude"]);

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
    }

    #[test]
    fn test_load_config_file_invalid_yaml() {
        let invalid_yaml = "invalid: yaml: content: [unclosed";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", invalid_yaml).unwrap();

        let result = ConfigManager::load_config_file(temp_file.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse config file"));
    }

    #[test]
    fn test_load_config_file_nonexistent() {
        let result = ConfigManager::load_config_file("/nonexistent/path/config.yaml");
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_load_from_env_invalid_values() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();

        // Clear any existing env vars
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");

        // Set invalid environment variables
        env::set_var("DURABLE_MAX_DEPTH", "not_a_number");
        env::set_var("DURABLE_FOLLOW_SYMLINKS", "not_a_bool");

        let config = ConfigManager::load_from_env_only().unwrap();

        // Invalid values should be ignored, keeping defaults
        assert_eq!(config.max_depth, Some(10)); // default
        assert!(!config.follow_symlinks); // default

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
    }

    #[test]
    fn test_config_precedence_env_over_file() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();

        // Clear any existing env vars
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");

        // Set valid env vars that should override file
        env::set_var("DURABLE_MAX_DEPTH", "20");
        env::set_var("DURABLE_EXCLUDE_PATTERNS", "env_pattern");

        let yaml_content = r#"
max_depth: 15
exclude_patterns:
  - file_pattern
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", yaml_content).unwrap();

        let config = ConfigManager::load_from_path(temp_file.path()).unwrap();

        // Should load from env (overrides file)
        assert_eq!(config.max_depth, Some(20));
        assert_eq!(config.exclude_patterns, vec!["env_pattern"]);

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
    }

    #[test]
    fn test_partial_config_file() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();

        // Clear any existing env vars
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");

        let yaml_content = r#"
max_depth: 30
# exclude_patterns not specified
project_indicators:
  - custom_indicator
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", yaml_content).unwrap();

        let config = ConfigManager::load_from_path(temp_file.path()).unwrap();

        assert_eq!(config.max_depth, Some(30));
        // exclude_patterns should keep defaults since not specified in file
        assert!(config
            .exclude_patterns
            .contains(&"node_modules".to_string()));
        assert_eq!(config.project_indicators, vec!["custom_indicator"]);

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
    }

    #[test]
    fn test_empty_config_file() {
        // Backup existing env vars
        let max_depth_backup = env::var("DURABLE_MAX_DEPTH").ok();
        let exclude_patterns_backup = env::var("DURABLE_EXCLUDE_PATTERNS").ok();
        let project_indicators_backup = env::var("DURABLE_PROJECT_INDICATORS").ok();
        let follow_symlinks_backup = env::var("DURABLE_FOLLOW_SYMLINKS").ok();

        // Clear all env vars first
        env::remove_var("DURABLE_MAX_DEPTH");
        env::remove_var("DURABLE_EXCLUDE_PATTERNS");
        env::remove_var("DURABLE_PROJECT_INDICATORS");
        env::remove_var("DURABLE_FOLLOW_SYMLINKS");

        // Set env vars to defaults where needed
        env::set_var("DURABLE_MAX_DEPTH", "10");
        env::set_var("DURABLE_FOLLOW_SYMLINKS", "false");

        let yaml_content = "{}";

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", yaml_content).unwrap();

        let config = ConfigManager::load_from_path(temp_file.path()).unwrap();

        // Should use all defaults
        assert_eq!(config.max_depth, Some(10));
        assert!(!config.follow_symlinks);

        // Restore env vars
        match max_depth_backup {
            Some(val) => env::set_var("DURABLE_MAX_DEPTH", val),
            None => env::remove_var("DURABLE_MAX_DEPTH"),
        }
        match exclude_patterns_backup {
            Some(val) => env::set_var("DURABLE_EXCLUDE_PATTERNS", val),
            None => env::remove_var("DURABLE_EXCLUDE_PATTERNS"),
        }
        match project_indicators_backup {
            Some(val) => env::set_var("DURABLE_PROJECT_INDICATORS", val),
            None => env::remove_var("DURABLE_PROJECT_INDICATORS"),
        }
        match follow_symlinks_backup {
            Some(val) => env::set_var("DURABLE_FOLLOW_SYMLINKS", val),
            None => env::remove_var("DURABLE_FOLLOW_SYMLINKS"),
        }
    }

    #[test]
    fn test_config_paths_include_expected_locations() {
        let paths = ConfigManager::get_config_paths();

        // Should include current directory files
        assert!(paths.iter().any(|p| p.ends_with("config.yaml")));
        assert!(paths.iter().any(|p| p.ends_with(".durable.yaml")));

        // Should include config directory paths if dirs crate can find them
        // (This might not be testable in all environments, so we'll just check the structure)
        assert!(paths.len() >= 2); // At minimum current directory files
    }

    #[test]
    fn test_config_validation() {
        // Valid config should pass
        let mut config = ScanConfig::default();
        assert!(ConfigManager::validate_config(&config).is_ok());

        // Invalid max_depth = 0
        config.max_depth = Some(0);
        assert!(ConfigManager::validate_config(&config).is_err());

        // Invalid max_depth > 1000
        config.max_depth = Some(1001);
        assert!(ConfigManager::validate_config(&config).is_err());

        // Reset to valid
        config.max_depth = Some(10);

        // Invalid empty exclude pattern
        config.exclude_patterns = vec!["valid".to_string(), "".to_string()];
        assert!(ConfigManager::validate_config(&config).is_err());

        // Invalid empty project indicator
        config.exclude_patterns = vec!["valid".to_string()]; // reset
        config.project_indicators = vec!["valid".to_string(), "   ".to_string()]; // whitespace only
        assert!(ConfigManager::validate_config(&config).is_err());
    }
}
