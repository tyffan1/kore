use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ContentScript {
    pub matches: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
    #[serde(default)]
    pub css: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackgroundConfig {
    pub scripts: Vec<String>,
    #[serde(default)]
    pub persistent: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserAction {
    #[serde(default)]
    pub default_title: Option<String>,
    #[serde(default)]
    pub default_popup: Option<String>,
    #[serde(default)]
    pub default_icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub manifest_version: u8,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub background: Option<BackgroundConfig>,
    #[serde(default)]
    pub content_scripts: Vec<ContentScript>,
    #[serde(default)]
    pub browser_action: Option<BrowserAction>,
    #[serde(default)]
    pub icons: Option<std::collections::HashMap<String, String>>,
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let m: Manifest = serde_json::from_str(json)?;
        if m.manifest_version != 2 {
            return Err(ManifestError::UnsupportedVersion(m.manifest_version));
        }
        if m.name.is_empty() {
            return Err(ManifestError::MissingField("name"));
        }
        if m.version.is_empty() {
            return Err(ManifestError::MissingField("version"));
        }
        Ok(m)
    }

    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Parse(String),
    UnsupportedVersion(u8),
    MissingField(&'static str),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Parse(msg) => write!(f, "manifest parse error: {msg}"),
            ManifestError::UnsupportedVersion(v) => {
                write!(f, "unsupported manifest version: {v} (only v2 is supported)")
            }
            ManifestError::MissingField(field) => {
                write!(f, "manifest missing required field: {field}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

impl From<serde_json::Error> for ManifestError {
    fn from(e: serde_json::Error) -> Self {
        ManifestError::Parse(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest_v2() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "Test Extension",
            "version": "1.0.0",
            "description": "A test",
            "permissions": ["tabs", "storage"]
        }"#;
        let m = Manifest::parse(json)?;
        assert_eq!(m.manifest_version, 2);
        assert_eq!(m.name, "Test Extension");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.description, "A test");
        assert_eq!(m.permissions, vec!["tabs".to_string(), "storage".to_string()]);
        Ok(())
    }

    #[test]
    fn rejects_unsupported_version() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test",
            "version": "1.0"
        }"#;
        let result = Manifest::parse(json);
        assert!(result.is_err());
        assert!(matches!(result, Err(ManifestError::UnsupportedVersion(3))));
    }

    #[test]
    fn rejects_missing_name() {
        let json = r#"{
            "manifest_version": 2,
            "name": "",
            "version": "1.0"
        }"#;
        let result = Manifest::parse(json);
        assert!(matches!(result, Err(ManifestError::MissingField("name"))));
    }

    #[test]
    fn rejects_missing_version() {
        let json = r#"{
            "manifest_version": 2,
            "name": "Test",
            "version": ""
        }"#;
        let result = Manifest::parse(json);
        assert!(matches!(result, Err(ManifestError::MissingField("version"))));
    }

    #[test]
    fn parses_background_scripts() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "Bg",
            "version": "1.0",
            "background": {
                "scripts": ["bg.js"],
                "persistent": true
            }
        }"#;
        let m = Manifest::parse(json)?;
        let bg = m.background.as_ref().ok_or(ManifestError::MissingField("background"))?;
        assert_eq!(bg.scripts, vec!["bg.js"]);
        assert!(bg.persistent);
        Ok(())
    }

    #[test]
    fn parses_content_scripts() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "CS",
            "version": "1.0",
            "content_scripts": [{
                "matches": ["https://*/*"],
                "js": ["content.js"],
                "css": ["style.css"]
            }]
        }"#;
        let m = Manifest::parse(json)?;
        assert_eq!(m.content_scripts.len(), 1);
        assert_eq!(m.content_scripts[0].matches, vec!["https://*/*"]);
        assert_eq!(m.content_scripts[0].js, vec!["content.js"]);
        assert_eq!(m.content_scripts[0].css, vec!["style.css"]);
        Ok(())
    }

    #[test]
    fn parses_browser_action() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "BA",
            "version": "1.0",
            "browser_action": {
                "default_title": "Click me",
                "default_popup": "popup.html"
            }
        }"#;
        let m = Manifest::parse(json)?;
        let ba = m.browser_action.as_ref().ok_or(ManifestError::MissingField("browser_action"))?;
        assert_eq!(ba.default_title.as_deref(), Some("Click me"));
        assert_eq!(ba.default_popup.as_deref(), Some("popup.html"));
        Ok(())
    }

    #[test]
    fn defaults_optional_fields() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "Min",
            "version": "0.1"
        }"#;
        let m = Manifest::parse(json)?;
        assert!(m.description.is_empty());
        assert!(m.permissions.is_empty());
        assert!(m.background.is_none());
        assert!(m.content_scripts.is_empty());
        assert!(m.browser_action.is_none());
        assert!(m.icons.is_none());
        Ok(())
    }

    #[test]
    fn has_permission_checks_correctly() -> Result<(), ManifestError> {
        let json = r#"{
            "manifest_version": 2,
            "name": "Perms",
            "version": "1.0",
            "permissions": ["tabs", "storage", "webRequest"]
        }"#;
        let m = Manifest::parse(json)?;
        assert!(m.has_permission("tabs"));
        assert!(m.has_permission("storage"));
        assert!(m.has_permission("webRequest"));
        assert!(!m.has_permission("bookmarks"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_json() {
        let result = Manifest::parse("not valid json");
        assert!(matches!(result, Err(ManifestError::Parse(_))));
    }
}
