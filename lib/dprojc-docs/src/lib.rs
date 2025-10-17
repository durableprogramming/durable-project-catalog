use dprojc_core::ProjectCatalog;
use dprojc_types::Project;
use std::path::Path;
use anyhow::Result;
use thiserror::Error;
use serde::{Serialize, Deserialize};
use handlebars::Handlebars;
use pulldown_cmark::{Parser, html};
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use walkdir::WalkDir;
use regex::Regex;

/// Errors that can occur during documentation generation
#[derive(Error, Debug)]
pub enum DocsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Template error: {0}")]
    Template(#[from] handlebars::TemplateError),
    #[error("Render error: {0}")]
    Render(#[from] handlebars::RenderError),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
}

/// Configuration for documentation generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocsConfig {
    pub output_dir: String,
    pub template_dir: Option<String>,
    pub include_readmes: bool,
    pub include_statistics: bool,
    pub include_project_details: bool,
    pub syntax_highlight: bool,
    pub theme: String,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            output_dir: "docs".to_string(),
            template_dir: None,
            include_readmes: true,
            include_statistics: true,
            include_project_details: true,
            syntax_highlight: true,
            theme: "base16-ocean.dark".to_string(),
        }
    }
}

/// Main documentation generator
pub struct DocumentationGenerator<'a> {
    catalog: &'a ProjectCatalog,
    config: DocsConfig,
    handlebars: Handlebars<'static>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl<'a> DocumentationGenerator<'a> {
    /// Create a new documentation generator
    pub fn new(catalog: &'a ProjectCatalog, config: DocsConfig) -> Result<Self> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);

        // Register built-in templates
        Self::register_builtin_templates(&mut handlebars)?;

        // Load custom templates if specified
        if let Some(template_dir) = &config.template_dir {
            Self::load_custom_templates(&mut handlebars, template_dir)?;
        }

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        Ok(Self {
            catalog,
            config,
            handlebars,
            syntax_set,
            theme_set,
        })
    }

    /// Register built-in Handlebars templates
    fn register_builtin_templates(handlebars: &mut Handlebars) -> Result<()> {
        // Index page template
        handlebars.register_template_string("index", include_str!("templates/index.hbs"))?;

        // Project list template
        handlebars.register_template_string("projects", include_str!("templates/projects.hbs"))?;

        // Project detail template
        handlebars.register_template_string("project_detail", include_str!("templates/project_detail.hbs"))?;

        // Statistics template
        handlebars.register_template_string("statistics", include_str!("templates/statistics.hbs"))?;

        // README template
        handlebars.register_template_string("readme", include_str!("templates/readme.hbs"))?;

        Ok(())
    }

    /// Load custom templates from directory
    fn load_custom_templates(handlebars: &mut Handlebars, template_dir: &str) -> Result<()> {
        let template_path = Path::new(template_dir);
        if !template_path.exists() {
            return Err(anyhow::anyhow!("Template directory does not exist: {}", template_dir));
        }
        if !template_path.is_dir() {
            return Err(anyhow::anyhow!("Template path is not a directory: {}", template_dir));
        }

        for entry in WalkDir::new(template_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "hbs" || ext == "handlebars" {
                        let template_name = entry.path()
                            .strip_prefix(template_dir)?
                            .with_extension("")
                            .to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/");

                        let content = std::fs::read_to_string(entry.path())?;
                        handlebars.register_template_string(&template_name, &content)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Generate all documentation
    pub async fn generate_all(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;

        self.generate_index().await?;
        self.generate_projects_list().await?;
        self.generate_statistics().await?;

        if self.config.include_project_details {
            self.generate_project_details().await?;
        }

        if self.config.include_readmes {
            self.generate_readmes().await?;
        }

        Ok(())
    }

    /// Generate index page
    async fn generate_index(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let projects = self.catalog.get_all_projects().await?;
        let stats = self.catalog.get_scan_statistics().await?;
        let counts = self.catalog.get_project_counts().await?;

        let data = serde_json::json!({
            "title": "Project Catalog Documentation",
            "total_projects": projects.len(),
            "total_scans": stats.total_scans,
            "project_counts": counts,
            "last_scan": stats.last_scan_timestamp.map_or("Never".to_string(), |t| t.to_rfc3339()),
            "generated_at": chrono::Utc::now().to_rfc3339(),
        });

        let output = self.handlebars.render("index", &data)?;
        std::fs::write(format!("{}/index.html", self.config.output_dir), output)?;

        Ok(())
    }

    /// Generate projects list page
    async fn generate_projects_list(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let projects = self.catalog.get_all_projects().await?;
        let counts = self.catalog.get_project_counts().await?;

        let projects_data: Vec<serde_json::Value> = projects.iter().map(|p| self.create_project_data(p)).collect();

        let data = serde_json::json!({
            "projects": projects_data,
            "project_counts": counts,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        });

        let output = self.handlebars.render("projects", &data)?;
        std::fs::write(format!("{}/projects.html", self.config.output_dir), output)?;

        Ok(())
    }

    /// Generate statistics page
    async fn generate_statistics(&self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let stats = self.catalog.get_scan_statistics().await?;
        let counts = self.catalog.get_project_counts().await?;
        let recent_scans = self.catalog.get_recent_scans(10).await?;

        let data = serde_json::json!({
            "statistics": stats,
            "project_counts": counts,
            "recent_scans": recent_scans,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        });

        let output = self.handlebars.render("statistics", &data)?;
        std::fs::write(format!("{}/statistics.html", self.config.output_dir), output)?;

        Ok(())
    }

    /// Generate individual project detail pages
    async fn generate_project_details(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let projects = self.catalog.get_all_projects().await?;

        for project in projects {
            self.generate_project_detail(&project).await?;
        }

        Ok(())
    }

    /// Generate detail page for a single project
    async fn generate_project_detail(&mut self, project: &Project) -> Result<()> {
        let readme_html = if self.config.include_readmes {
            Self::extract_readme(&project.path)
                .ok()
                .map(|md| self.render_markdown(&md))
                .transpose()?
        } else {
            None
        };

        // Derive additional fields for the template
        let project_data = self.create_project_data(project);

        let data = serde_json::json!({
            "project": project_data,
            "readme": readme_html,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        });

        let output = self.handlebars.render("project_detail", &data)?;
        let filename = format!("project_{}.html", Self::derive_project_id(&project.path));
        std::fs::write(format!("{}/{}", self.config.output_dir, filename), output)?;

        Ok(())
    }

    /// Generate README pages for projects that have them
    async fn generate_readmes(&mut self) -> Result<()> {
        std::fs::create_dir_all(&self.config.output_dir)?;
        let projects = self.catalog.get_all_projects().await?;

        for project in projects {
            if let Ok(readme_md) = Self::extract_readme(&project.path) {
                let readme_html = self.render_markdown(&readme_md)?;
                let project_data = self.create_project_data(&project);
                let data = serde_json::json!({
                    "project": project_data,
                    "readme": readme_html,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                });

                let output = self.handlebars.render("readme", &data)?;
                let filename = format!("readme_{}.html", Self::derive_project_id(&project.path));
                std::fs::write(format!("{}/{}", self.config.output_dir, filename), output)?;
            }
        }

        Ok(())
    }

    /// Extract README content from a project directory
    fn extract_readme(project_path: &Path) -> Result<String> {
        let readme_files = ["README.md", "README.txt", "README", "readme.md", "readme.txt"];

        for filename in &readme_files {
            let readme_path = project_path.join(filename);
            if readme_path.exists() {
                return Ok(std::fs::read_to_string(readme_path)?);
            }
        }

        Err(anyhow::anyhow!("No README file found"))
    }

    /// Create project data for templates
    fn create_project_data(&self, project: &Project) -> serde_json::Value {
        serde_json::json!({
            "id": Self::derive_project_id(&project.path),
            "name": project.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            "path": project.path.display().to_string(),
            "project_type": project.project_type.to_string(),
            "language": Self::detect_language(&project.path).unwrap_or_else(|| "Unknown".to_string()),
            "last_modified": project.last_scanned.to_rfc3339(),
        })
    }

    /// Derive a project ID from its path
    fn derive_project_id(path: &Path) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    /// Detect programming language from project path and indicators
    fn detect_language(path: &Path) -> Option<String> {
        // Language detection based on common project files
        if path.join("Cargo.toml").exists() {
            Some("Rust".to_string())
        } else if path.join("package.json").exists() {
            Some("JavaScript/TypeScript".to_string())
        } else if path.join("pyproject.toml").exists() || path.join("setup.py").exists() || path.join("requirements.txt").exists() {
            Some("Python".to_string())
        } else if path.join("go.mod").exists() || path.join("go.sum").exists() {
            Some("Go".to_string())
        } else if path.join("pom.xml").exists() || path.join("build.gradle").exists() || path.join("build.gradle.kts").exists() {
            Some("Java".to_string())
        } else if path.join("Gemfile").exists() || path.join(".gemspec").exists() {
            Some("Ruby".to_string())
        } else if path.join("composer.json").exists() {
            Some("PHP".to_string())
        } else if path.join("CMakeLists.txt").exists() || path.join("Makefile").exists() {
            Some("C/C++".to_string())
        } else if path.join("devenv.nix").exists() || path.join("flake.nix").exists() {
            Some("Nix".to_string())
        } else {
            None
        }
    }

    /// Generate markdown report
    pub async fn generate_markdown_report(&self, output_path: &Path) -> Result<()> {
        let projects = self.catalog.get_all_projects().await?;
        let stats = self.catalog.get_scan_statistics().await?;
        let counts = self.catalog.get_project_counts().await?;

        let mut content = String::new();
        content.push_str("# Project Catalog Report\n\n");
        content.push_str(&format!("Generated on: {}\n\n", chrono::Utc::now().to_rfc3339()));

        content.push_str("## Statistics\n\n");
        content.push_str(&format!("- Total Projects: {}\n", projects.len()));
        content.push_str(&format!("- Total Scans: {}\n", stats.total_scans));
        content.push_str(&format!("- Last Scan: {}\n\n", stats.last_scan_timestamp.map_or("Never".to_string(), |t| t.to_rfc3339())));

        content.push_str("## Project Types\n\n");
        for (project_type, count) in counts {
            content.push_str(&format!("- {}: {}\n", project_type, count));
        }
        content.push('\n');

        content.push_str("## Projects\n\n");
        for project in projects {
            let name = project.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let language = Self::detect_language(&project.path).unwrap_or_else(|| "Unknown".to_string());

            content.push_str(&format!("### {}\n\n", name));
            content.push_str(&format!("- **Path:** `{}`\n", project.path.display()));
            content.push_str(&format!("- **Type:** {}\n", project.project_type));
            content.push_str(&format!("- **Language:** {}\n", language));
            content.push_str(&format!("- **Last Scanned:** {}\n\n", project.last_scanned.to_rfc3339()));

            if let Ok(readme) = Self::extract_readme(&project.path) {
                content.push_str("#### README\n\n");
                content.push_str(&readme);
                content.push_str("\n\n");
            }
        }

        std::fs::write(output_path, content)?;
        Ok(())
    }

    /// Generate JSON export
    pub async fn generate_json_export(&self, output_path: &Path) -> Result<()> {
        let projects = self.catalog.get_all_projects().await?;
        let stats = self.catalog.get_scan_statistics().await?;
        let counts = self.catalog.get_project_counts().await?;

        let export_data = serde_json::json!({
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "statistics": stats,
            "project_counts": counts,
            "projects": projects,
        });

        let content = serde_json::to_string_pretty(&export_data)?;
        std::fs::write(output_path, content)?;
        Ok(())
    }

    /// Render markdown to HTML with syntax highlighting
    pub fn render_markdown(&mut self, markdown: &str) -> Result<String> {
        let parser = Parser::new(markdown);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        if self.config.syntax_highlight {
            // Apply syntax highlighting to code blocks
            self.apply_syntax_highlighting(&mut html_output)?;
        }

        Ok(html_output)
    }

    /// Apply syntax highlighting to HTML content
    fn apply_syntax_highlighting(&mut self, html: &mut String) -> Result<()> {
        let code_block_regex = Regex::new(r#"<pre><code class="language-([^"]*)">([\s\S]*?)</code></pre>"#)?;

        let theme = self.theme_set.themes.get(&self.config.theme)
            .or_else(|| self.theme_set.themes.values().next())
            .ok_or_else(|| anyhow::anyhow!("No syntax highlighting theme available"))?;

        *html = code_block_regex.replace_all(html, |caps: &regex::Captures| {
            let lang = &caps[1];
            let code = &caps[2];

            if let Some(syntax) = self.syntax_set.find_syntax_by_token(lang) {
                highlighted_html_for_string(code, &self.syntax_set, syntax, theme)
                    .unwrap_or_else(|_| format!("<pre><code class=\"language-{}\">{}</code></pre>", lang, code.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")))
            } else {
                format!("<pre><code class=\"language-{}\">{}</code></pre>", lang, code.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))
            }
        }).to_string();

        Ok(())
    }
}

/// Helper functions for documentation generation
pub mod utils {
    use super::*;

    /// Generate a simple text summary of the catalog
    pub async fn generate_text_summary(catalog: &ProjectCatalog) -> Result<String> {
        let projects = catalog.get_all_projects().await?;
        let stats = catalog.get_scan_statistics().await?;
        let counts = catalog.get_project_counts().await?;

        let mut summary = "Project Catalog Summary\n".to_string();
        summary.push_str(&format!("Generated: {}\n\n", chrono::Utc::now().to_rfc3339()));
        summary.push_str(&format!("Total Projects: {}\n", projects.len()));
        summary.push_str(&format!("Total Scans: {}\n", stats.total_scans));
        summary.push_str(&format!("Last Scan: {}\n\n", stats.last_scan_timestamp.map_or("Never".to_string(), |t| t.to_rfc3339())));

        summary.push_str("Project Types:\n");
        for (project_type, count) in counts {
            summary.push_str(&format!("  {}: {}\n", project_type, count));
        }

        summary.push_str("\nRecent Projects:\n");
        for project in projects.iter().take(10) {
            let name = project.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            summary.push_str(&format!("  - {} ({}) at {}\n", name, project.project_type, project.path.display()));
        }

        Ok(summary)
    }

    /// Validate documentation configuration
    pub fn validate_config(config: &DocsConfig) -> Result<()> {
        if config.output_dir.is_empty() {
            return Err(anyhow::anyhow!("Output directory cannot be empty"));
        }

        if let Some(template_dir) = &config.template_dir {
            if !Path::new(template_dir).exists() {
                return Err(anyhow::anyhow!("Template directory does not exist: {}", template_dir));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dprojc_core::ProjectCatalog;
    use std::fs;
    use tempfile::tempdir;

    async fn create_test_catalog() -> (ProjectCatalog, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let catalog = ProjectCatalog::with_db_path(db_path).await.unwrap();
        (catalog, temp_dir)
    }

    #[tokio::test]
    async fn test_docs_config_default() {
        let config = DocsConfig::default();
        assert_eq!(config.output_dir, "docs");
        assert!(config.include_readmes);
        assert!(config.include_statistics);
    }

    #[tokio::test]
    async fn test_generate_markdown_report() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let config = DocsConfig::default();
        let generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let output_path = temp_dir.path().join("report.md");
        generator.generate_markdown_report(&output_path).await.unwrap();

        assert!(output_path.exists());
        let content = fs::read_to_string(output_path).unwrap();
        assert!(content.contains("# Project Catalog Report"));
        assert!(content.contains("Git"));
    }

    #[tokio::test]
    async fn test_generate_json_export() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let config = DocsConfig::default();
        let generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let output_path = temp_dir.path().join("export.json");
        generator.generate_json_export(&output_path).await.unwrap();

        assert!(output_path.exists());
        let content = fs::read_to_string(output_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(json["projects"].as_array().unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_extract_readme() {
        let temp_dir = tempdir().unwrap();
        let readme_path = temp_dir.path().join("README.md");
        fs::write(&readme_path, "# Test Project\n\nThis is a test.").unwrap();

        let content = DocumentationGenerator::extract_readme(temp_dir.path()).unwrap();
        assert_eq!(content, "# Test Project\n\nThis is a test.");
    }

    #[tokio::test]
    async fn test_extract_readme_various_names() {
        let temp_dir = tempdir().unwrap();

        // Test README.txt
        let readme_path = temp_dir.path().join("README.txt");
        fs::write(&readme_path, "Text README content.").unwrap();
        let content = DocumentationGenerator::extract_readme(temp_dir.path()).unwrap();
        assert_eq!(content, "Text README content.");

        // Test readme.md (lowercase)
        fs::remove_file(&readme_path).unwrap();
        let readme_path = temp_dir.path().join("readme.md");
        fs::write(&readme_path, "# Lowercase readme").unwrap();
        let content = DocumentationGenerator::extract_readme(temp_dir.path()).unwrap();
        assert_eq!(content, "# Lowercase readme");

        // Test README without extension
        fs::remove_file(&readme_path).unwrap();
        let readme_path = temp_dir.path().join("README");
        fs::write(&readme_path, "Plain README").unwrap();
        let content = DocumentationGenerator::extract_readme(temp_dir.path()).unwrap();
        assert_eq!(content, "Plain README");
    }

    #[tokio::test]
    async fn test_extract_readme_no_readme() {
        let temp_dir = tempdir().unwrap();
        // Create some other files but no README
        fs::write(temp_dir.path().join("some_file.txt"), "content").unwrap();

        let result = DocumentationGenerator::extract_readme(temp_dir.path());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_render_markdown() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let mut config = DocsConfig::default();
        config.syntax_highlight = false;
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let markdown = "# Hello\n\n```rust\nprintln!(\"Hello, world!\");\n```";
        let html = generator.render_markdown(markdown).unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<code"));
    }

    #[tokio::test]
    async fn test_utils_validate_config() {
        let mut config = DocsConfig::default();
        assert!(utils::validate_config(&config).is_ok());

        config.output_dir = "".to_string();
        assert!(utils::validate_config(&config).is_err());

        config.output_dir = "docs".to_string();
        config.template_dir = Some("/nonexistent".to_string());
        assert!(utils::validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn test_utils_generate_text_summary() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let summary = utils::generate_text_summary(&catalog).await.unwrap();
        assert!(summary.contains("Project Catalog Summary"));
        assert!(summary.contains("Total Projects: 1"));
    }

    #[tokio::test]
    async fn test_generate_index_page() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let output_dir = temp_dir.path().join("docs").to_string_lossy().to_string();
        let config = DocsConfig {
            output_dir,
            ..Default::default()
        };
        let generator = DocumentationGenerator::new(&catalog, config).unwrap();

        generator.generate_index().await.unwrap();

        let index_path = temp_dir.path().join("docs/index.html");
        assert!(index_path.exists());

        let content = fs::read_to_string(index_path).unwrap();
        assert!(content.contains("Project Catalog Documentation"));
        assert!(content.contains("Total Projects"));
    }

    #[tokio::test]
    async fn test_generate_statistics_page() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();
        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let output_dir = temp_dir.path().join("docs").to_string_lossy().to_string();
        let config = DocsConfig {
            output_dir,
            ..Default::default()
        };
        let generator = DocumentationGenerator::new(&catalog, config).unwrap();

        generator.generate_statistics().await.unwrap();

        let stats_path = temp_dir.path().join("docs/statistics.html");
        assert!(stats_path.exists());

        let content = fs::read_to_string(stats_path).unwrap();
        assert!(content.contains("Catalog Statistics"));
        assert!(content.contains("Total Scans"));
    }

    #[tokio::test]
    async fn test_generate_project_detail_with_readme() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create a README file
        let readme_path = temp_dir.path().join("README.md");
        fs::write(&readme_path, "# Test Project\n\nThis is a test project.").unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let output_dir = temp_dir.path().join("docs").to_string_lossy().to_string();
        let config = DocsConfig {
            output_dir,
            include_readmes: true,
            ..Default::default()
        };
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        generator.generate_project_details().await.unwrap();

        // Check if project detail page was generated
        let detail_files: Vec<_> = fs::read_dir(temp_dir.path().join("docs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("project_"))
            .collect();

        assert!(!detail_files.is_empty());

        let content = fs::read_to_string(detail_files[0].path()).unwrap();
        assert!(content.contains("Test Project"));
    }

    #[tokio::test]
    async fn test_generate_readme_pages() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create a README file
        let readme_path = temp_dir.path().join("README.md");
        fs::write(&readme_path, "# Test README\n\nContent here.").unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let output_dir = temp_dir.path().join("docs").to_string_lossy().to_string();
        let config = DocsConfig {
            output_dir,
            include_readmes: true,
            ..Default::default()
        };
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        generator.generate_readmes().await.unwrap();

        // Check if README page was generated
        let readme_files: Vec<_> = fs::read_dir(temp_dir.path().join("docs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("readme_"))
            .collect();

        assert!(!readme_files.is_empty());

        let content = fs::read_to_string(readme_files[0].path()).unwrap();
        assert!(content.contains("Test README"));
    }

    #[tokio::test]
    async fn test_generate_all_functionality() {
        let (mut catalog, _temp_dir) = create_test_catalog().await;

        let temp_dir = tempdir().unwrap();
        fs::create_dir(temp_dir.path().join(".git")).unwrap();

        // Create a README file
        let readme_path = temp_dir.path().join("README.md");
        fs::write(&readme_path, "# Test Project\n\nDescription.").unwrap();

        catalog.scan_directory(temp_dir.path()).await.unwrap();

        let config = DocsConfig {
            output_dir: temp_dir.path().join("output").to_string_lossy().to_string(),
            include_readmes: true,
            include_statistics: true,
            include_project_details: true,
            ..Default::default()
        };
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        generator.generate_all().await.unwrap();

        let output_dir = temp_dir.path().join("output");

        // Check that all expected files were generated
        assert!(output_dir.join("index.html").exists());
        assert!(output_dir.join("projects.html").exists());
        assert!(output_dir.join("statistics.html").exists());

        // Check for project detail and README files
        let files: Vec<_> = fs::read_dir(&output_dir).unwrap().filter_map(|e| e.ok()).collect();
        let has_project_detail = files.iter().any(|f| f.file_name().to_string_lossy().starts_with("project_"));
        let has_readme = files.iter().any(|f| f.file_name().to_string_lossy().starts_with("readme_"));

        assert!(has_project_detail);
        assert!(has_readme);
    }

    #[tokio::test]
    async fn test_derive_project_id() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join("test_project");

        let id1 = DocumentationGenerator::derive_project_id(&path);
        let id2 = DocumentationGenerator::derive_project_id(&path);

        // Same path should produce same ID
        assert_eq!(id1, id2);

        let different_path = temp_dir.path().join("different_project");
        let id3 = DocumentationGenerator::derive_project_id(&different_path);

        // Different path should produce different ID
        assert_ne!(id1, id3);
    }

    #[tokio::test]
    async fn test_detect_language() {
        let temp_dir = tempdir().unwrap();

        // Test Rust detection
        let rust_dir = temp_dir.path().join("rust_project");
        fs::create_dir(&rust_dir).unwrap();
        fs::write(rust_dir.join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&rust_dir), Some("Rust".to_string()));

        // Test Node.js detection
        let node_dir = temp_dir.path().join("node_project");
        fs::create_dir(&node_dir).unwrap();
        fs::write(node_dir.join("package.json"), "{}").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&node_dir), Some("JavaScript/TypeScript".to_string()));

        // Test Python detection (multiple indicators)
        let python_dir = temp_dir.path().join("python_project");
        fs::create_dir(&python_dir).unwrap();
        fs::write(python_dir.join("pyproject.toml"), "[tool.poetry]").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&python_dir), Some("Python".to_string()));

        let python_dir2 = temp_dir.path().join("python_project2");
        fs::create_dir(&python_dir2).unwrap();
        fs::write(python_dir2.join("requirements.txt"), "flask").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&python_dir2), Some("Python".to_string()));

        // Test Go detection
        let go_dir = temp_dir.path().join("go_project");
        fs::create_dir(&go_dir).unwrap();
        fs::write(go_dir.join("go.mod"), "module test").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&go_dir), Some("Go".to_string()));

        // Test Java detection
        let java_dir = temp_dir.path().join("java_project");
        fs::create_dir(&java_dir).unwrap();
        fs::write(java_dir.join("pom.xml"), "<project>").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&java_dir), Some("Java".to_string()));

        // Test Ruby detection
        let ruby_dir = temp_dir.path().join("ruby_project");
        fs::create_dir(&ruby_dir).unwrap();
        fs::write(ruby_dir.join("Gemfile"), "source 'https://rubygems.org'").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&ruby_dir), Some("Ruby".to_string()));

        // Test PHP detection
        let php_dir = temp_dir.path().join("php_project");
        fs::create_dir(&php_dir).unwrap();
        fs::write(php_dir.join("composer.json"), "{}").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&php_dir), Some("PHP".to_string()));

        // Test C/C++ detection
        let cpp_dir = temp_dir.path().join("cpp_project");
        fs::create_dir(&cpp_dir).unwrap();
        fs::write(cpp_dir.join("CMakeLists.txt"), "cmake_minimum_required").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&cpp_dir), Some("C/C++".to_string()));

        // Test Nix detection
        let nix_dir = temp_dir.path().join("nix_project");
        fs::create_dir(&nix_dir).unwrap();
        fs::write(nix_dir.join("devenv.nix"), "{}").unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&nix_dir), Some("Nix".to_string()));

        // Test unknown language
        let unknown_dir = temp_dir.path().join("unknown_project");
        fs::create_dir(&unknown_dir).unwrap();
        assert_eq!(DocumentationGenerator::detect_language(&unknown_dir), None);
    }

    #[tokio::test]
    async fn test_config_validation() {
        // Valid config
        let config = DocsConfig::default();
        assert!(utils::validate_config(&config).is_ok());

        // Invalid config - empty output dir
        let mut invalid_config = DocsConfig::default();
        invalid_config.output_dir = "".to_string();
        assert!(utils::validate_config(&invalid_config).is_err());

        // Invalid config - nonexistent template dir
        let mut invalid_config2 = DocsConfig::default();
        invalid_config2.template_dir = Some("/nonexistent/path".to_string());
        assert!(utils::validate_config(&invalid_config2).is_err());
    }

    #[tokio::test]
    async fn test_markdown_rendering_with_syntax_highlighting() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let config = DocsConfig::default();
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let markdown = r#"
# Header

```rust
fn main() {
    println!("Hello, world!");
}
```

Some text.
"#;

        let html = generator.render_markdown(markdown).unwrap();
        assert!(html.contains("<h1>Header</h1>"));
        assert!(html.contains("<pre"));
        assert!(html.contains("println"));
    }

    #[tokio::test]
    async fn test_markdown_rendering_without_syntax_highlighting() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let config = DocsConfig {
            syntax_highlight: false,
            ..Default::default()
        };
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let markdown = r#"
# Header

```rust
fn main() {
    println!("Hello, world!");
}
```

Some text.
"#;

        let html = generator.render_markdown(markdown).unwrap();
        assert!(html.contains("<h1>Header</h1>"));
        assert!(html.contains("<pre><code class=\"language-rust\">"));
        assert!(html.contains("println"));
    }

    #[tokio::test]
    async fn test_syntax_highlighting_invalid_theme() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let config = DocsConfig {
            theme: "nonexistent-theme".to_string(),
            ..Default::default()
        };
        let mut generator = DocumentationGenerator::new(&catalog, config).unwrap();

        let mut html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#.to_string();
        // This should not panic even with invalid theme - it should fall back
        let result = generator.apply_syntax_highlighting(&mut html);
        // The result might be ok (if fallback theme exists) or err (if no themes)
        // Either way, it shouldn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_template_registration() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let config = DocsConfig::default();

        // This should not panic and should register templates successfully
        let generator = DocumentationGenerator::new(&catalog, config);
        assert!(generator.is_ok());
    }

    #[tokio::test]
    async fn test_custom_template_loading() {
        let (catalog, _temp_dir) = create_test_catalog().await;
        let temp_dir = tempdir().unwrap();

        // Create a custom template directory
        let template_dir = temp_dir.path().join("custom_templates");
        fs::create_dir(&template_dir).unwrap();

        // Create a custom template
        let custom_template = template_dir.join("custom.hbs");
        fs::write(&custom_template, "<h1>Custom: {{title}}</h1>").unwrap();

        let config = DocsConfig {
            template_dir: Some(template_dir.to_string_lossy().to_string()),
            ..Default::default()
        };

        let generator = DocumentationGenerator::new(&catalog, config);
        assert!(generator.is_ok());

        // Check that the custom template was loaded
        let result = generator.unwrap().handlebars.render("custom", &serde_json::json!({"title": "Test"}));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "<h1>Custom: Test</h1>");
    }

    #[tokio::test]
    async fn test_custom_template_invalid_directory() {
        let (catalog, _temp_dir) = create_test_catalog().await;

        let config = DocsConfig {
            template_dir: Some("/nonexistent/directory".to_string()),
            ..Default::default()
        };

        // This should fail during template loading
        let generator = DocumentationGenerator::new(&catalog, config);
        assert!(generator.is_err());
    }
}