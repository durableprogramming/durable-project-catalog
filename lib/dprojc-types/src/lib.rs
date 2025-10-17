use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a discovered software project
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    /// Absolute path to the project directory
    pub path: PathBuf,
    /// Type of project based on detected indicators
    pub project_type: ProjectType,
    /// List of indicators found in this project
    pub indicators: Vec<ProjectIndicator>,
    /// Timestamp when the project was last scanned
    pub last_scanned: chrono::DateTime<chrono::Utc>,
}

/// Types of projects that can be detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProjectType {
    Git,
    NodeJs,
    Ruby,
    Rust,
    Python,
    Go,
    Java,
    Nix,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Git => write!(f, "Git"),
            ProjectType::NodeJs => write!(f, "Node.js"),
            ProjectType::Ruby => write!(f, "Ruby"),
            ProjectType::Rust => write!(f, "Rust"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Java => write!(f, "Java"),
            ProjectType::Nix => write!(f, "Nix"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Indicators that identify a project type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProjectIndicator {
    GitDirectory,
    PackageJson,
    Gemfile,
    Gemspec,
    CargoToml,
    PyprojectToml,
    RequirementsTxt,
    GoMod,
    PomXml,
    DevenvNix,
    Custom(String),
}

impl std::fmt::Display for ProjectIndicator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectIndicator::GitDirectory => write!(f, ".git"),
            ProjectIndicator::PackageJson => write!(f, "package.json"),
            ProjectIndicator::Gemfile => write!(f, "Gemfile"),
            ProjectIndicator::Gemspec => write!(f, ".gemspec"),
            ProjectIndicator::CargoToml => write!(f, "Cargo.toml"),
            ProjectIndicator::PyprojectToml => write!(f, "pyproject.toml"),
            ProjectIndicator::RequirementsTxt => write!(f, "requirements.txt"),
            ProjectIndicator::GoMod => write!(f, "go.mod"),
            ProjectIndicator::PomXml => write!(f, "pom.xml"),
            ProjectIndicator::DevenvNix => write!(f, "devenv.nix"),
            ProjectIndicator::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Result of a directory scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Root path that was scanned
    pub root_path: PathBuf,
    /// Projects found during the scan
    pub projects: Vec<Project>,
    /// Directories that were excluded from scanning
    pub excluded_dirs: Vec<PathBuf>,
    /// Errors encountered during scanning
    pub errors: Vec<ScanError>,
    /// Total directories scanned
    pub dirs_scanned: usize,
    /// Scan duration in milliseconds
    pub scan_duration_ms: u64,
}

/// Errors that can occur during scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanError {
    pub path: PathBuf,
    pub error_type: ScanErrorType,
    pub message: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} ({})",
            self.path.display(),
            self.message,
            self.error_type
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[repr(u8)]
pub enum ScanErrorType {
    PermissionDenied,
    PathNotFound,
    IoError,
    Other,
}

impl std::fmt::Display for ScanErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanErrorType::PermissionDenied => write!(f, "Permission Denied"),
            ScanErrorType::PathNotFound => write!(f, "Path Not Found"),
            ScanErrorType::IoError => write!(f, "IO Error"),
            ScanErrorType::Other => write!(f, "Other Error"),
        }
    }
}

/// Configuration for the scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    /// Maximum depth to scan
    pub max_depth: Option<usize>,
    /// Patterns to exclude from scanning
    pub exclude_patterns: Vec<String>,
    /// Additional project indicators to check
    pub project_indicators: Vec<String>,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            max_depth: Some(10),
            exclude_patterns: vec![
                "node_modules".to_string(),
                "vendor".to_string(),
                ".git".to_string(),
                "__pycache__".to_string(),
                "target".to_string(),
                "build".to_string(),
                "dist".to_string(),
            ],
            project_indicators: vec![
                ".git".to_string(),
                "package.json".to_string(),
                "Gemfile".to_string(),
                ".gemspec".to_string(),
                "Cargo.toml".to_string(),
                "pyproject.toml".to_string(),
                "requirements.txt".to_string(),
                "go.mod".to_string(),
                "pom.xml".to_string(),
                "devenv.nix".to_string(),
            ],
            follow_symlinks: false,
        }
    }
}

impl ProjectType {
    /// Determine project type from indicators
    pub fn from_indicators(indicators: &[ProjectIndicator]) -> Self {
        if indicators.contains(&ProjectIndicator::CargoToml) {
            ProjectType::Rust
        } else if indicators.contains(&ProjectIndicator::PackageJson) {
            ProjectType::NodeJs
        } else if indicators.contains(&ProjectIndicator::Gemfile)
            || indicators.contains(&ProjectIndicator::Gemspec)
        {
            ProjectType::Ruby
        } else if indicators.contains(&ProjectIndicator::PyprojectToml)
            || indicators.contains(&ProjectIndicator::RequirementsTxt)
        {
            ProjectType::Python
        } else if indicators.contains(&ProjectIndicator::GoMod) {
            ProjectType::Go
        } else if indicators.contains(&ProjectIndicator::PomXml) {
            ProjectType::Java
        } else if indicators.contains(&ProjectIndicator::GitDirectory) {
            ProjectType::Git
        } else if indicators.contains(&ProjectIndicator::DevenvNix) {
            ProjectType::Nix
        } else {
            ProjectType::Unknown
        }
    }
}

impl ProjectIndicator {
    /// Convert a file/directory name to an indicator
    pub fn from_path_name(name: &str) -> Option<Self> {
        match name {
            ".git" => Some(ProjectIndicator::GitDirectory),
            "package.json" => Some(ProjectIndicator::PackageJson),
            "Gemfile" => Some(ProjectIndicator::Gemfile),
            ".gemspec" => Some(ProjectIndicator::Gemspec),
            "Cargo.toml" => Some(ProjectIndicator::CargoToml),
            "pyproject.toml" => Some(ProjectIndicator::PyprojectToml),
            "requirements.txt" => Some(ProjectIndicator::RequirementsTxt),
            "go.mod" => Some(ProjectIndicator::GoMod),
            "pom.xml" => Some(ProjectIndicator::PomXml),
            "devenv.nix" => Some(ProjectIndicator::DevenvNix),
            _ => None,
        }
    }
}

/// Summary of a scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResultSummary {
    pub id: i64,
    pub scan_timestamp: chrono::DateTime<chrono::Utc>,
    pub root_path: std::path::PathBuf,
    pub dirs_scanned: usize,
    pub scan_duration_ms: u64,
    pub error_count: usize,
    pub excluded_dirs_count: usize,
}

/// Scan statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStatistics {
    pub total_scans: usize,
    pub total_projects: usize,
    pub total_dirs_scanned: usize,
    pub total_errors: usize,
    pub last_scan_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Data structure for reports
#[derive(serde::Serialize)]
pub struct ReportData {
    pub projects: Vec<Project>,
    pub statistics: Option<ScanStatistics>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

/// Data structure for statistics
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StatsData {
    pub statistics: ScanStatistics,
    pub project_counts: std::collections::HashMap<ProjectType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_project_type_from_indicators() {
        // Test single indicators
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::CargoToml]),
            ProjectType::Rust
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::PackageJson]),
            ProjectType::NodeJs
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::Gemfile]),
            ProjectType::Ruby
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::Gemspec]),
            ProjectType::Ruby
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::PyprojectToml]),
            ProjectType::Python
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::RequirementsTxt]),
            ProjectType::Python
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::GoMod]),
            ProjectType::Go
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::PomXml]),
            ProjectType::Java
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::GitDirectory]),
            ProjectType::Git
        );
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::DevenvNix]),
            ProjectType::Nix
        );

        // Test precedence (CargoToml should take precedence over others)
        assert_eq!(
            ProjectType::from_indicators(&[
                ProjectIndicator::PackageJson,
                ProjectIndicator::CargoToml
            ]),
            ProjectType::Rust
        );
        assert_eq!(
            ProjectType::from_indicators(&[
                ProjectIndicator::GitDirectory,
                ProjectIndicator::PackageJson
            ]),
            ProjectType::NodeJs
        );

        // Test unknown/custom indicators
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::Custom("unknown".to_string())]),
            ProjectType::Unknown
        );

        // Test empty indicators
        assert_eq!(ProjectType::from_indicators(&[]), ProjectType::Unknown);

        // Test multiple indicators of same type
        assert_eq!(
            ProjectType::from_indicators(&[ProjectIndicator::Gemfile, ProjectIndicator::Gemspec]),
            ProjectType::Ruby
        );
    }

    #[test]
    fn test_project_indicator_from_path_name() {
        // Test known indicators
        assert_eq!(
            ProjectIndicator::from_path_name(".git"),
            Some(ProjectIndicator::GitDirectory)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("package.json"),
            Some(ProjectIndicator::PackageJson)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("Gemfile"),
            Some(ProjectIndicator::Gemfile)
        );
        assert_eq!(
            ProjectIndicator::from_path_name(".gemspec"),
            Some(ProjectIndicator::Gemspec)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("Cargo.toml"),
            Some(ProjectIndicator::CargoToml)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("pyproject.toml"),
            Some(ProjectIndicator::PyprojectToml)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("go.mod"),
            Some(ProjectIndicator::GoMod)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("requirements.txt"),
            Some(ProjectIndicator::RequirementsTxt)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("pom.xml"),
            Some(ProjectIndicator::PomXml)
        );
        assert_eq!(
            ProjectIndicator::from_path_name("devenv.nix"),
            Some(ProjectIndicator::DevenvNix)
        );

        // Test unknown names
        assert_eq!(ProjectIndicator::from_path_name("unknown.txt"), None);
        assert_eq!(ProjectIndicator::from_path_name(""), None);
        assert_eq!(ProjectIndicator::from_path_name("random_file"), None);
    }

    #[test]
    fn test_scan_config_default() {
        let config = ScanConfig::default();

        assert_eq!(config.max_depth, Some(10));
        assert!(config
            .exclude_patterns
            .contains(&"node_modules".to_string()));
        assert!(config.exclude_patterns.contains(&"vendor".to_string()));
        assert!(config.exclude_patterns.contains(&".git".to_string()));
        assert!(config.exclude_patterns.contains(&"__pycache__".to_string()));
        assert!(config.exclude_patterns.contains(&"target".to_string()));
        assert!(config.exclude_patterns.contains(&"build".to_string()));
        assert!(config.exclude_patterns.contains(&"dist".to_string()));
        assert_eq!(config.exclude_patterns.len(), 7);

        assert!(config.project_indicators.contains(&".git".to_string()));
        assert!(config
            .project_indicators
            .contains(&"package.json".to_string()));
        assert!(config.project_indicators.contains(&"Gemfile".to_string()));
        assert!(config.project_indicators.contains(&".gemspec".to_string()));
        assert!(config
            .project_indicators
            .contains(&"Cargo.toml".to_string()));
        assert!(config
            .project_indicators
            .contains(&"pyproject.toml".to_string()));
        assert!(config.project_indicators.contains(&"go.mod".to_string()));
        assert!(config.project_indicators.contains(&"pom.xml".to_string()));
        assert!(config
            .project_indicators
            .contains(&"devenv.nix".to_string()));
        assert!(config
            .project_indicators
            .contains(&"requirements.txt".to_string()));
        assert_eq!(config.project_indicators.len(), 10);

        assert_eq!(config.follow_symlinks, false);
    }

    #[test]
    fn test_serialization_project() {
        let project = Project {
            path: std::path::PathBuf::from("/test/path"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml, ProjectIndicator::GitDirectory],
            last_scanned: chrono::Utc::now(),
        };

        let serialized = serde_json::to_string(&project).unwrap();
        let deserialized: Project = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.path, project.path);
        assert_eq!(deserialized.project_type, project.project_type);
        assert_eq!(deserialized.indicators, project.indicators);
        // Note: last_scanned might have slight differences due to serialization precision
    }

    #[test]
    fn test_serialization_scan_result() {
        let scan_result = ScanResult {
            root_path: std::path::PathBuf::from("/test/root"),
            projects: vec![],
            excluded_dirs: vec![std::path::PathBuf::from("/test/excluded")],
            errors: vec![ScanError {
                path: std::path::PathBuf::from("/test/error"),
                error_type: ScanErrorType::IoError,
                message: "Test error".to_string(),
            }],
            dirs_scanned: 42,
            scan_duration_ms: 1000,
        };

        let serialized = serde_json::to_string(&scan_result).unwrap();
        let deserialized: ScanResult = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.root_path, scan_result.root_path);
        assert_eq!(deserialized.projects.len(), 0);
        assert_eq!(deserialized.excluded_dirs, scan_result.excluded_dirs);
        assert_eq!(deserialized.errors.len(), 1);
        assert_eq!(deserialized.errors[0].path, scan_result.errors[0].path);
        assert_eq!(
            deserialized.errors[0].error_type,
            scan_result.errors[0].error_type
        );
        assert_eq!(
            deserialized.errors[0].message,
            scan_result.errors[0].message
        );
        assert_eq!(deserialized.dirs_scanned, scan_result.dirs_scanned);
        assert_eq!(deserialized.scan_duration_ms, scan_result.scan_duration_ms);
    }

    #[test]
    fn test_serialization_scan_config() {
        let config = ScanConfig {
            max_depth: Some(5),
            exclude_patterns: vec!["test".to_string()],
            project_indicators: vec!["indicator".to_string()],
            follow_symlinks: true,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: ScanConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.max_depth, config.max_depth);
        assert_eq!(deserialized.exclude_patterns, config.exclude_patterns);
        assert_eq!(deserialized.project_indicators, config.project_indicators);
        assert_eq!(deserialized.follow_symlinks, config.follow_symlinks);
    }

    #[test]
    fn test_serialization_scan_error() {
        let error = ScanError {
            path: std::path::PathBuf::from("/test/path"),
            error_type: ScanErrorType::PermissionDenied,
            message: "Permission denied".to_string(),
        };

        let serialized = serde_json::to_string(&error).unwrap();
        let deserialized: ScanError = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.path, error.path);
        assert_eq!(deserialized.error_type, error.error_type);
        assert_eq!(deserialized.message, error.message);
    }

    #[test]
    fn test_hash_eq_implementations() {
        // Test ProjectType Hash and Eq
        let mut set = HashSet::new();
        set.insert(ProjectType::Rust);
        set.insert(ProjectType::NodeJs);
        set.insert(ProjectType::Rust); // Duplicate should not be added
        assert_eq!(set.len(), 2);

        // Test ProjectIndicator Hash and Eq
        let mut indicator_set = HashSet::new();
        indicator_set.insert(ProjectIndicator::CargoToml);
        indicator_set.insert(ProjectIndicator::PackageJson);
        indicator_set.insert(ProjectIndicator::CargoToml); // Duplicate
        assert_eq!(indicator_set.len(), 2);

        // Test custom indicators
        indicator_set.insert(ProjectIndicator::Custom("test".to_string()));
        indicator_set.insert(ProjectIndicator::Custom("test".to_string())); // Duplicate
        assert_eq!(indicator_set.len(), 3);
    }

    #[test]
    fn test_edge_cases() {
        // Test empty indicators list
        assert_eq!(ProjectType::from_indicators(&[]), ProjectType::Unknown);

        // Test unknown path names
        assert_eq!(ProjectIndicator::from_path_name(""), None);
        assert_eq!(ProjectIndicator::from_path_name("file.with.dots.txt"), None);
        assert_eq!(ProjectIndicator::from_path_name("UPPERCASE"), None);

        // Test ScanConfig with None max_depth
        let config = ScanConfig {
            max_depth: None,
            exclude_patterns: vec![],
            project_indicators: vec![],
            follow_symlinks: false,
        };
        assert_eq!(config.max_depth, None);

        // Test ScanError with empty message
        let error = ScanError {
            path: std::path::PathBuf::from(""),
            error_type: ScanErrorType::Other,
            message: String::new(),
        };
        assert_eq!(error.message, "");
    }

    #[test]
    fn test_project_indicator_custom() {
        let custom = ProjectIndicator::Custom("my_custom_indicator".to_string());
        match custom {
            ProjectIndicator::Custom(name) => assert_eq!(name, "my_custom_indicator"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_scan_error_types() {
        assert_eq!(ScanErrorType::PermissionDenied as u8, 0);
        assert_eq!(ScanErrorType::PathNotFound as u8, 1);
        assert_eq!(ScanErrorType::IoError as u8, 2);
        assert_eq!(ScanErrorType::Other as u8, 3);
    }

    #[test]
    fn test_display_implementations() {
        // Test ProjectType Display
        assert_eq!(format!("{}", ProjectType::Rust), "Rust");
        assert_eq!(format!("{}", ProjectType::NodeJs), "Node.js");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");

        // Test ScanErrorType Display
        assert_eq!(
            format!("{}", ScanErrorType::PermissionDenied),
            "Permission Denied"
        );
        assert_eq!(format!("{}", ScanErrorType::IoError), "IO Error");
        assert_eq!(format!("{}", ScanErrorType::Other), "Other Error");

        // Test ProjectIndicator Display
        assert_eq!(format!("{}", ProjectIndicator::CargoToml), "Cargo.toml");
        assert_eq!(format!("{}", ProjectIndicator::PackageJson), "package.json");
        assert_eq!(
            format!("{}", ProjectIndicator::RequirementsTxt),
            "requirements.txt"
        );
        assert_eq!(
            format!("{}", ProjectIndicator::Custom("test".to_string())),
            "test"
        );

        // Test ScanError Display
        let error = ScanError {
            path: std::path::PathBuf::from("/test/path"),
            error_type: ScanErrorType::PermissionDenied,
            message: "Access denied".to_string(),
        };
        assert_eq!(
            format!("{}", error),
            "/test/path: Access denied (Permission Denied)"
        );
    }

    #[test]
    fn test_project_types_exhaustive() {
        // Ensure all project types are covered
        let types = vec![
            ProjectType::Git,
            ProjectType::NodeJs,
            ProjectType::Ruby,
            ProjectType::Rust,
            ProjectType::Python,
            ProjectType::Go,
            ProjectType::Java,
            ProjectType::Nix,
            ProjectType::Unknown,
        ];
        assert_eq!(types.len(), 9);
    }
}
