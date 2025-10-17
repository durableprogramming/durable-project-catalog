//! Output formatting utilities

use std::io::{self, Write};
use comfy_table::Table;
use dprojc_types::{Project, ScanResult, StatsData, ReportData};
use dprojc_utils::format_path_display;

/// Output format enum
#[derive(Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format
    Table,
    /// JSON format
    Json,
    /// YAML format
    Yaml,
}

/// Output formatter
pub struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn format_projects(&self, projects: &[Project]) -> anyhow::Result<()> {
        self.format_projects_to_writer(projects, &mut io::stdout())
    }

    pub fn format_projects_to_writer<W: Write>(&self, projects: &[Project], writer: &mut W) -> anyhow::Result<()> {
        match self.format {
            OutputFormat::Table => self.format_projects_table(projects, writer),
            OutputFormat::Json => self.format_projects_json(projects, writer),
            OutputFormat::Yaml => self.format_projects_yaml(projects, writer),
        }
    }

    pub fn format_scan_results(&self, results: &[ScanResult]) -> anyhow::Result<()> {
        self.format_scan_results_to_writer(results, &mut io::stdout())
    }

    pub fn format_scan_results_to_writer<W: Write>(&self, results: &[ScanResult], writer: &mut W) -> anyhow::Result<()> {
        match self.format {
            OutputFormat::Table => self.format_scan_results_table(results, writer),
            OutputFormat::Json => self.format_scan_results_json(results, writer),
            OutputFormat::Yaml => self.format_scan_results_yaml(results, writer),
        }
    }

    pub fn format_stats(&self, stats: &StatsData) -> anyhow::Result<()> {
        self.format_stats_to_writer(stats, &mut io::stdout())
    }

    pub fn format_stats_to_writer<W: Write>(&self, stats: &StatsData, writer: &mut W) -> anyhow::Result<()> {
        match self.format {
            OutputFormat::Table => self.format_stats_table(stats, writer),
            OutputFormat::Json => self.format_stats_json(stats, writer),
            OutputFormat::Yaml => self.format_stats_yaml(stats, writer),
        }
    }

    pub fn format_report(&self, report: &ReportData) -> anyhow::Result<()> {
        self.format_report_to_writer(report, &mut io::stdout())
    }

    pub fn format_report_to_writer<W: Write>(&self, report: &ReportData, writer: &mut W) -> anyhow::Result<()> {
        match self.format {
            OutputFormat::Table => self.format_report_table(report, writer),
            OutputFormat::Json => self.format_report_json(report, writer),
            OutputFormat::Yaml => self.format_report_yaml(report, writer),
        }
    }
}

impl OutputFormatter {
    fn format_projects_table<W: Write>(&self, projects: &[Project], writer: &mut W) -> anyhow::Result<()> {
        if projects.is_empty() {
            writeln!(writer, "No projects found.")?;
            return Ok(());
        }

        let mut table = Table::new();
        table.set_header(vec![
            "Path",
            "Type",
            "Indicators",
            "Last Scanned"
        ]);

        for project in projects {
            let path = format_path_display(&project.path);
            let project_type = format!("{}", project.project_type);
            let indicators: Vec<String> = project.indicators.iter()
                .map(|i| format!("{}", i))
                .collect();
            let indicators_str = indicators.join(", ");
            let last_scanned = project.last_scanned.format("%Y-%m-%d %H:%M:%S");

            table.add_row(vec![
                path,
                project_type,
                indicators_str,
                last_scanned.to_string(),
            ]);
        }

        writeln!(writer, "{}", table)?;
        Ok(())
    }

    fn format_scan_results_table<W: Write>(&self, results: &[ScanResult], writer: &mut W) -> anyhow::Result<()> {
        for result in results {
            writeln!(writer, "Scan Results for: {}", result.root_path.display())?;
            writeln!(writer, "Directories scanned: {}", result.dirs_scanned)?;
            writeln!(writer, "Projects found: {}", result.projects.len())?;
            writeln!(writer, "Errors: {}", result.errors.len())?;
            if !result.excluded_dirs.is_empty() {
                writeln!(writer, "Excluded directories: {}", result.excluded_dirs.len())?;
                for dir in &result.excluded_dirs {
                    writeln!(writer, "  {}", format_path_display(dir))?;
                }
            } else {
                writeln!(writer, "Excluded directories: 0")?;
            }
            writeln!(writer, "Scan duration: {}ms", result.scan_duration_ms)?;
            writeln!(writer)?;

            if !result.projects.is_empty() {
                self.format_projects_table(&result.projects, writer)?;
                writeln!(writer)?;
            }

            if !result.errors.is_empty() {
                writeln!(writer, "Errors:")?;
                for error in &result.errors {
                    writeln!(writer, "  {}: {}", error.path.display(), error.message)?;
                }
                writeln!(writer)?;
            }
        }
        Ok(())
    }

    fn format_stats_table<W: Write>(&self, stats: &StatsData, writer: &mut W) -> anyhow::Result<()> {
        let mut table = Table::new();
        table.set_header(vec!["Statistic", "Value"]);

        table.add_row(vec!["Total Scans", &stats.statistics.total_scans.to_string()]);
        table.add_row(vec!["Total Projects", &stats.statistics.total_projects.to_string()]);
        table.add_row(vec!["Total Directories Scanned", &stats.statistics.total_dirs_scanned.to_string()]);
        table.add_row(vec!["Total Errors", &stats.statistics.total_errors.to_string()]);

        if let Some(last_scan) = stats.statistics.last_scan_timestamp {
            table.add_row(vec!["Last Scan", &last_scan.format("%Y-%m-%d %H:%M:%S").to_string()]);
        } else {
            table.add_row(vec!["Last Scan", "Never"]);
        }

        writeln!(writer, "{}", table)?;
        writeln!(writer)?;

        // Project counts by type
        if !stats.project_counts.is_empty() {
            writeln!(writer, "Projects by Type:")?;
            let mut type_table = Table::new();
            type_table.set_header(vec!["Type", "Count"]);

            for (project_type, count) in &stats.project_counts {
                type_table.add_row(vec![format!("{}", project_type), count.to_string()]);
            }

            writeln!(writer, "{}", type_table)?;
        }

        Ok(())
    }

    fn format_projects_json<W: Write>(&self, projects: &[Project], writer: &mut W) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut *writer, projects)?;
        writeln!(writer)?;
        Ok(())
    }

    fn format_scan_results_json<W: Write>(&self, results: &[ScanResult], writer: &mut W) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut *writer, results)?;
        writeln!(writer)?;
        Ok(())
    }

    fn format_stats_json<W: Write>(&self, stats: &StatsData, writer: &mut W) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut *writer, stats)?;
        writeln!(writer)?;
        Ok(())
    }

    fn format_projects_yaml<W: Write>(&self, projects: &[Project], writer: &mut W) -> anyhow::Result<()> {
        serde_yaml::to_writer(writer, projects)?;
        Ok(())
    }

    fn format_scan_results_yaml<W: Write>(&self, results: &[ScanResult], writer: &mut W) -> anyhow::Result<()> {
        serde_yaml::to_writer(writer, results)?;
        Ok(())
    }

    fn format_stats_yaml<W: Write>(&self, stats: &StatsData, writer: &mut W) -> anyhow::Result<()> {
        serde_yaml::to_writer(writer, stats)?;
        Ok(())
    }

    fn format_report_table<W: Write>(&self, report: &ReportData, writer: &mut W) -> anyhow::Result<()> {
        writeln!(writer, "Report generated at: {}", report.generated_at.format("%Y-%m-%d %H:%M:%S"))?;
        writeln!(writer)?;

        if !report.projects.is_empty() {
            writeln!(writer, "Projects:")?;
            self.format_projects_table(&report.projects, writer)?;
            writeln!(writer)?;
        } else {
            writeln!(writer, "No projects found.")?;
            writeln!(writer)?;
        }

        if let Some(stats) = &report.statistics {
            writeln!(writer, "Statistics:")?;
            let mut table = Table::new();
            table.set_header(vec!["Statistic", "Value"]);

            table.add_row(vec!["Total Scans", &stats.total_scans.to_string()]);
            table.add_row(vec!["Total Projects", &stats.total_projects.to_string()]);
            table.add_row(vec!["Total Directories Scanned", &stats.total_dirs_scanned.to_string()]);
            table.add_row(vec!["Total Errors", &stats.total_errors.to_string()]);

            if let Some(last_scan) = stats.last_scan_timestamp {
                table.add_row(vec!["Last Scan", &last_scan.format("%Y-%m-%d %H:%M:%S").to_string()]);
            } else {
                table.add_row(vec!["Last Scan", "Never"]);
            }

            writeln!(writer, "{}", table)?;
        }

        Ok(())
    }

    fn format_report_json<W: Write>(&self, report: &ReportData, writer: &mut W) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut *writer, report)?;
        writeln!(writer)?;
        Ok(())
    }

    fn format_report_yaml<W: Write>(&self, report: &ReportData, writer: &mut W) -> anyhow::Result<()> {
        serde_yaml::to_writer(writer, report)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dprojc_types::{Project, ProjectType, ProjectIndicator};
    use std::path::PathBuf;

    fn create_test_projects() -> Vec<Project> {
        vec![
            Project {
                path: PathBuf::from("/test/rust_project"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: PathBuf::from("/test/node_project"),
                project_type: ProjectType::NodeJs,
                indicators: vec![ProjectIndicator::PackageJson],
                last_scanned: chrono::Utc::now(),
            },
        ]
    }

    fn create_test_stats_data() -> StatsData {
        StatsData {
            statistics: dprojc_types::ScanStatistics {
                total_scans: 1,
                total_projects: 2,
                total_dirs_scanned: 50,
                total_errors: 0,
                last_scan_timestamp: Some(chrono::Utc::now()),
            },
            project_counts: std::collections::HashMap::from([
                (ProjectType::Rust, 1),
                (ProjectType::NodeJs, 1),
            ]),
        }
    }

    fn create_test_scan_results() -> Vec<ScanResult> {
        vec![
            ScanResult {
                root_path: PathBuf::from("/test/root"),
                projects: create_test_projects(),
                excluded_dirs: vec![PathBuf::from("/test/root/node_modules"), PathBuf::from("/test/root/.git")],
                errors: vec![dprojc_types::ScanError {
                    path: PathBuf::from("/test/root/forbidden"),
                    error_type: dprojc_types::ScanErrorType::PermissionDenied,
                    message: "Permission denied".to_string(),
                }],
                dirs_scanned: 10,
                scan_duration_ms: 500,
            }
        ]
    }

    #[test]
    fn test_output_formatter_table() {
        let formatter = OutputFormatter::new(OutputFormat::Table);
        let projects = create_test_projects();

        let mut output = Vec::new();
        formatter.format_projects_to_writer(&projects, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
        assert!(output_str.contains("Rust"));
        assert!(output_str.contains("Node.js"));
    }

    #[test]
    fn test_output_formatter_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let projects = create_test_projects();

        let mut output = Vec::new();
        formatter.format_projects_to_writer(&projects, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
        assert!(output_str.contains("\"Rust\""));
        assert!(output_str.contains("\"NodeJs\""));
    }

    #[test]
    fn test_output_formatter_yaml() {
        let formatter = OutputFormatter::new(OutputFormat::Yaml);
        let projects = create_test_projects();

        let mut output = Vec::new();
        formatter.format_projects_to_writer(&projects, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
    }

    #[test]
    fn test_output_formatter_empty_projects() {
        let formatter = OutputFormatter::new(OutputFormat::Table);
        let projects: Vec<Project> = vec![];

        let mut output = Vec::new();
        formatter.format_projects_to_writer(&projects, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("No projects found"));
    }

    #[test]
    fn test_output_formatter_stats() {
        let formatter = OutputFormatter::new(OutputFormat::Table);
        let stats = create_test_stats_data();

        let mut output = Vec::new();
        formatter.format_stats_to_writer(&stats, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Total Scans"));
        assert!(output_str.contains("Total Projects"));
        assert!(output_str.contains("1"));
        assert!(output_str.contains("2"));
    }

    #[test]
    fn test_output_formatter_scan_results_table() {
        let formatter = OutputFormatter::new(OutputFormat::Table);
        let results = create_test_scan_results();

        let mut output = Vec::new();
        formatter.format_scan_results_to_writer(&results, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Scan Results for: /test/root"));
        assert!(output_str.contains("Directories scanned: 10"));
        assert!(output_str.contains("Projects found: 2"));
        assert!(output_str.contains("Errors: 1"));
        assert!(output_str.contains("Excluded directories: 2"));
        assert!(output_str.contains("/test/root/node_modules"));
        assert!(output_str.contains("/test/root/.git"));
        assert!(output_str.contains("Scan duration: 500ms"));
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
    }

    #[test]
    fn test_output_formatter_scan_results_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let results = create_test_scan_results();

        let mut output = Vec::new();
        formatter.format_scan_results_to_writer(&results, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("/test/root"));
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
        assert!(output_str.contains("Permission denied"));
    }

    #[test]
    fn test_output_formatter_scan_results_yaml() {
        let formatter = OutputFormatter::new(OutputFormat::Yaml);
        let results = create_test_scan_results();

        let mut output = Vec::new();
        formatter.format_scan_results_to_writer(&results, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("/test/root"));
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
    }

    fn create_test_report_data() -> ReportData {
        ReportData {
            projects: create_test_projects(),
            statistics: Some(dprojc_types::ScanStatistics {
                total_scans: 1,
                total_projects: 2,
                total_dirs_scanned: 50,
                total_errors: 0,
                last_scan_timestamp: Some(chrono::Utc::now()),
            }),
            generated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_output_formatter_report_table() {
        let formatter = OutputFormatter::new(OutputFormat::Table);
        let report = create_test_report_data();

        let mut output = Vec::new();
        formatter.format_report_to_writer(&report, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Report generated at:"));
        assert!(output_str.contains("Projects:"));
        assert!(output_str.contains("Statistics:"));
        assert!(output_str.contains("Total Scans"));
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
    }

    #[test]
    fn test_output_formatter_report_json() {
        let formatter = OutputFormatter::new(OutputFormat::Json);
        let report = create_test_report_data();

        let mut output = Vec::new();
        formatter.format_report_to_writer(&report, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("projects"));
        assert!(output_str.contains("statistics"));
        assert!(output_str.contains("generated_at"));
        assert!(output_str.contains("rust_project"));
        assert!(output_str.contains("node_project"));
    }

    #[test]
    fn test_output_formatter_report_yaml() {
        let formatter = OutputFormatter::new(OutputFormat::Yaml);
        let report = create_test_report_data();

        let mut output = Vec::new();
        formatter.format_report_to_writer(&report, &mut output).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("projects:"));
        assert!(output_str.contains("statistics:"));
        assert!(output_str.contains("generated_at:"));
    }

    #[test]
    fn test_output_format_enum() {
        // Test that OutputFormat implements the necessary traits
        let _table = OutputFormat::Table;
        let _json = OutputFormat::Json;
        let _yaml = OutputFormat::Yaml;
    }
}