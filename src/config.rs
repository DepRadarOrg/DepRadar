//! Project-level configuration read from `depradar.toml`.
//!
//! When the `depradar scan` command runs it looks for a `depradar.toml` file
//! in the working directory. If one is found, its values are used as defaults
//! for every flag that the file supports. CLI flags always win over file
//! values, and file values win over built-in defaults.
//!
//! # File format
//!
//! ```toml
//! # depradar.toml
//!
//! # Severity level at which the run exits with code 1.
//! # Accepted values: "low", "medium", "high", "critical", "none"
//! # Default: "high"
//! fail_on = "high"
//!
//! # Output format for the scan command.
//! # Accepted values: "text", "json"
//! # Default: "text"
//! format = "text"
//!
//! # Glob patterns for paths to skip. The patterns are matched against every
//! # lockfile path that depradar would otherwise scan.
//! # Default: []
//! exclude = ["vendor/**", "third_party/**"]
//!
//! # Restrict scanning to these ecosystems.
//! # Accepted values: "npm", "pypi", "cargo", "go", "maven", "rubygems", "packagist"
//! # Default: all ecosystems
//! ecosystems = ["npm", "pypi"]
//! ```

use std::fmt;
use std::path::Path;

use serde::Deserialize;

/// Name of the project config file depradar looks for in the working
/// directory.
pub const CONFIG_FILE_NAME: &str = "depradar.toml";

/// The severity level at which `depradar scan` exits with code 1.
///
/// The variants are ordered from least to most severe so that `>=` comparisons
/// work correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailOn {
    None,
    Low,
    Medium,
    #[default]
    High,
    Critical,
}

impl fmt::Display for FailOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FailOn::None => "none",
            FailOn::Low => "low",
            FailOn::Medium => "medium",
            FailOn::High => "high",
            FailOn::Critical => "critical",
        })
    }
}

impl std::str::FromStr for FailOn {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Ok(FailOn::None),
            "low" => Ok(FailOn::Low),
            "medium" => Ok(FailOn::Medium),
            "high" => Ok(FailOn::High),
            "critical" => Ok(FailOn::Critical),
            other => Err(format!(
                "unknown severity level `{other}`; expected one of: none, low, medium, high, critical"
            )),
        }
    }
}

/// An ecosystem name as accepted in `depradar.toml`.
///
/// Each variant maps to the corresponding [`crate::model::Ecosystem`] value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigEcosystem {
    Npm,
    #[serde(alias = "PyPI")]
    Pypi,
    Cargo,
    Go,
    Maven,
    #[serde(alias = "RubyGems")]
    Rubygems,
    Packagist,
}

impl fmt::Display for ConfigEcosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConfigEcosystem::Npm => "npm",
            ConfigEcosystem::Pypi => "pypi",
            ConfigEcosystem::Cargo => "cargo",
            ConfigEcosystem::Go => "go",
            ConfigEcosystem::Maven => "maven",
            ConfigEcosystem::Rubygems => "rubygems",
            ConfigEcosystem::Packagist => "packagist",
        })
    }
}

impl std::str::FromStr for ConfigEcosystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "npm" => Ok(ConfigEcosystem::Npm),
            "pypi" => Ok(ConfigEcosystem::Pypi),
            "cargo" => Ok(ConfigEcosystem::Cargo),
            "go" => Ok(ConfigEcosystem::Go),
            "maven" => Ok(ConfigEcosystem::Maven),
            "rubygems" => Ok(ConfigEcosystem::Rubygems),
            "packagist" => Ok(ConfigEcosystem::Packagist),
            other => Err(format!(
                "unknown ecosystem `{other}`; expected one of: npm, pypi, cargo, go, maven, rubygems, packagist"
            )),
        }
    }
}

/// The output format for `depradar scan`, as accepted in `depradar.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFormat {
    #[default]
    Text,
    Json,
}

impl fmt::Display for ConfigFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConfigFormat::Text => "text",
            ConfigFormat::Json => "json",
        })
    }
}

impl std::str::FromStr for ConfigFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "text" => Ok(ConfigFormat::Text),
            "json" => Ok(ConfigFormat::Json),
            other => Err(format!(
                "unknown format `{other}`; expected one of: text, json"
            )),
        }
    }
}

/// Contents of a `depradar.toml` file.
///
/// Every field is optional; missing fields fall back to the built-in default.
/// CLI flags override whichever fields they correspond to.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// Severity threshold for a non-zero exit code. `none` never fails.
    /// Default: `high`.
    pub fail_on: Option<FailOn>,

    /// Output format for the scan report. Default: `text`.
    pub format: Option<ConfigFormat>,

    /// Glob patterns for lockfile paths to skip. Paths are matched relative
    /// to the scanned root.
    #[serde(default)]
    pub exclude: Vec<String>,

    /// Restrict scans to these ecosystems. Empty (the default) means all
    /// ecosystems.
    #[serde(default)]
    pub ecosystems: Vec<ConfigEcosystem>,
}

/// Errors that can occur when loading a `depradar.toml`.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The file's contents are not valid TOML or do not match the expected
    /// schema.
    Parse(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(err) => write!(f, "could not read {CONFIG_FILE_NAME}: {err}"),
            ConfigError::Parse(err) => write!(f, "invalid {CONFIG_FILE_NAME}: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io(err) => Some(err),
            ConfigError::Parse(err) => Some(err),
        }
    }
}

/// Tries to load `depradar.toml` from `dir`.
///
/// Returns `Ok(None)` when no config file exists in `dir` — that is not an
/// error. Returns `Err` only when the file exists but cannot be read or
/// parsed.
pub fn load_project_config(dir: &Path) -> Result<Option<ProjectConfig>, ConfigError> {
    let path = dir.join(CONFIG_FILE_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path).map_err(ConfigError::Io)?;
    let config: ProjectConfig = toml::from_str(&contents).map_err(ConfigError::Parse)?;
    Ok(Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ── FailOn ────────────────────────────────────────────────────────────────

    #[test]
    fn fail_on_ordering() {
        assert!(FailOn::None < FailOn::Low);
        assert!(FailOn::Low < FailOn::Medium);
        assert!(FailOn::Medium < FailOn::High);
        assert!(FailOn::High < FailOn::Critical);
    }

    #[test]
    fn fail_on_round_trips() {
        for (s, expected) in [
            ("none", FailOn::None),
            ("low", FailOn::Low),
            ("medium", FailOn::Medium),
            ("high", FailOn::High),
            ("critical", FailOn::Critical),
        ] {
            let parsed = FailOn::from_str(s).unwrap();
            assert_eq!(parsed, expected, "from_str({s:?})");
            assert_eq!(parsed.to_string(), s, "Display({s:?})");
        }
    }

    #[test]
    fn fail_on_unknown_is_error() {
        assert!(FailOn::from_str("extreme").is_err());
    }

    // ── ConfigEcosystem ───────────────────────────────────────────────────────

    #[test]
    fn ecosystem_round_trips() {
        for (s, expected) in [
            ("npm", ConfigEcosystem::Npm),
            ("pypi", ConfigEcosystem::Pypi),
            ("cargo", ConfigEcosystem::Cargo),
            ("go", ConfigEcosystem::Go),
            ("maven", ConfigEcosystem::Maven),
            ("rubygems", ConfigEcosystem::Rubygems),
            ("packagist", ConfigEcosystem::Packagist),
        ] {
            let parsed = ConfigEcosystem::from_str(s).unwrap();
            assert_eq!(parsed, expected, "from_str({s:?})");
            assert_eq!(parsed.to_string(), s, "Display({s:?})");
        }
    }

    #[test]
    fn ecosystem_unknown_is_error() {
        assert!(ConfigEcosystem::from_str("maven2").is_err());
    }

    // ── ConfigFormat ──────────────────────────────────────────────────────────

    #[test]
    fn format_round_trips() {
        assert_eq!(ConfigFormat::from_str("text").unwrap(), ConfigFormat::Text);
        assert_eq!(ConfigFormat::from_str("json").unwrap(), ConfigFormat::Json);
        assert!(ConfigFormat::from_str("sarif").is_err());
    }

    // ── ProjectConfig parsing ─────────────────────────────────────────────────

    #[test]
    fn empty_toml_gives_default_config() {
        let cfg: ProjectConfig = toml::from_str("").unwrap();
        assert!(cfg.fail_on.is_none());
        assert!(cfg.format.is_none());
        assert!(cfg.exclude.is_empty());
        assert!(cfg.ecosystems.is_empty());
    }

    #[test]
    fn full_config_parses_correctly() {
        let toml = r#"
            fail_on = "medium"
            format = "json"
            exclude = ["vendor/**", "third_party/**"]
            ecosystems = ["npm", "pypi"]
        "#;
        let cfg: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.fail_on, Some(FailOn::Medium));
        assert_eq!(cfg.format, Some(ConfigFormat::Json));
        assert_eq!(cfg.exclude, vec!["vendor/**", "third_party/**"]);
        assert_eq!(
            cfg.ecosystems,
            vec![ConfigEcosystem::Npm, ConfigEcosystem::Pypi]
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let toml = r#"unknown_key = "oops""#;
        assert!(toml::from_str::<ProjectConfig>(toml).is_err());
    }

    #[test]
    fn invalid_fail_on_value_is_rejected() {
        let toml = r#"fail_on = "extreme""#;
        assert!(toml::from_str::<ProjectConfig>(toml).is_err());
    }

    // ── load_project_config ───────────────────────────────────────────────────

    #[test]
    fn returns_none_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_project_config(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn loads_file_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILE_NAME), r#"fail_on = "low""#).unwrap();
        let cfg = load_project_config(dir.path()).unwrap().unwrap();
        assert_eq!(cfg.fail_on, Some(FailOn::Low));
    }

    #[test]
    fn returns_error_for_malformed_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILE_NAME), "[[invalid{{").unwrap();
        assert!(load_project_config(dir.path()).is_err());
    }
}
