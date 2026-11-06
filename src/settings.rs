//! App-wide preferences and their platform-specific storage locations.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use toml_edit::{DocumentMut, Item, Table, value};

/// Stable identity shared by configuration and future local per-document data.
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "JonGretar";
const APPLICATION: &str = "Kver";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    config: PathBuf,
    local_data: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Option<Self> {
        let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)?;
        Some(Self {
            config: dirs.config_dir().join("config.toml"),
            local_data: dirs.data_local_dir().to_path_buf(),
        })
    }

    /// Explicit locations for isolated tests, without changing process-wide HOME.
    pub fn for_locations(config: PathBuf, local_data: PathBuf) -> Self {
        Self { config, local_data }
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn local_data(&self) -> &Path {
        &self.local_data
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn parse(key: &str) -> Result<Self, String> {
        match key {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err(format!(
                "Unknown appearance {key:?}; use system, light, or dark"
            )),
        }
    }
}

fn read_content(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "Cannot read settings at {}: {error}",
            path.display()
        )),
    }
}

fn parse_document(path: &Path, content: Option<&str>) -> Result<DocumentMut, String> {
    content
        .map(|content| {
            content
                .parse()
                .map_err(|error| format!("Cannot read settings at {}: {error}", path.display()))
        })
        .unwrap_or_else(|| Ok(DocumentMut::new()))
}

fn read_document(path: &Path) -> Result<DocumentMut, String> {
    parse_document(path, read_content(path)?.as_deref())
}

fn appearance_in(document: &DocumentMut) -> Result<Appearance, String> {
    let Some(appearance) = document.get("appearance") else {
        return Ok(Appearance::System);
    };
    let appearance = appearance
        .as_table()
        .ok_or("Settings [appearance] must be a table")?;
    match appearance.get("theme_mode") {
        None => Ok(Appearance::System),
        Some(theme_mode) => theme_mode
            .as_str()
            .ok_or_else(|| "Settings [appearance].theme_mode must be a string".to_owned())
            .and_then(Appearance::parse),
    }
}

pub fn load(path: &Path) -> Result<Appearance, String> {
    appearance_in(&read_document(path)?)
}

/// Re-read before writing so unrelated hand-edited TOML remains intact.
/// Refuse invalid input rather than replacing a file the user needs to repair.
pub fn save(path: &Path, appearance: Appearance) -> Result<(), String> {
    let original = read_content(path)?;
    let mut document = parse_document(path, original.as_deref())?;
    // Reject invalid or incompatible shapes rather than overwriting hand edits.
    appearance_in(&document)?;
    if document.get("appearance").is_none() {
        document["appearance"] = Item::Table(Table::new());
    }
    if let Some(existing) = document["appearance"]["theme_mode"].as_value_mut() {
        // Replacing an Item loses its inline comment; changing the value keeps its decor.
        let decor = existing.decor().clone();
        *existing = appearance.key().into();
        *existing.decor_mut() = decor;
    } else {
        document["appearance"]["theme_mode"] = value(appearance.key());
    }
    let parent = path
        .parent()
        .ok_or("Settings path has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Cannot create settings directory {}: {error}",
            parent.display()
        )
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Cannot save settings at {}: {error}", path.display()))?;
    use std::io::Write as _;
    temporary
        .write_all(document.to_string().as_bytes())
        .and_then(|()| temporary.flush())
        .map_err(|error| format!("Cannot save settings at {}: {error}", path.display()))?;
    // An editor may replace the file while the panel is preparing its write.
    // Never knowingly replace a newer version; the next choice can retry.
    if read_content(path)? != original {
        return Err(format!(
            "Settings changed at {} while saving; try the choice again",
            path.display()
        ));
    }
    temporary.persist(path).map_err(|error| {
        format!(
            "Cannot save settings at {}: {}",
            path.display(),
            error.error
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_defaults_without_creating_directories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/config.toml");
        assert_eq!(load(&path).unwrap(), Appearance::System);
        assert!(!path.exists());
    }

    #[test]
    fn choices_survive_restart_and_preserve_other_entries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            "# keep this\nfuture_setting = 7\n\n[appearance]\ntheme_mode = 'system' # keep inline\naccent = 'violet'\n\n[future]\nsetting = 9\n",
        )
        .unwrap();
        save(&path, Appearance::Dark).unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::Dark);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# keep this"));
        assert!(content.contains(" # keep inline"));
        assert!(content.contains("future_setting = 7"));
        assert!(content.contains("accent = 'violet'"));
        assert!(content.contains("[future]\nsetting = 9"));
        save(&path, Appearance::Light).unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::Light);
    }

    #[test]
    fn missing_section_gets_only_appearance_when_chosen() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/config.toml");
        save(&path, Appearance::Light).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[appearance]\ntheme_mode = \"light\"\n"
        );
        fs::write(&path, "[other]\nkey = 1\n").unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::System);
        save(&path, Appearance::Dark).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("[other]\nkey = 1")
        );
        assert_eq!(load(&path).unwrap(), Appearance::Dark);
    }

    #[test]
    fn invalid_or_inaccessible_config_is_not_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "[appearance]\ntheme_mode = 'purple'\n").unwrap();
        assert!(load(&path).is_err());
        assert!(save(&path, Appearance::Dark).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[appearance]\ntheme_mode = 'purple'\n"
        );
        fs::write(&path, "appearance = 'dark'\n").unwrap();
        assert!(load(&path).is_err());
        assert!(save(&path, Appearance::Dark).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "appearance = 'dark'\n");
        fs::write(&path, "appearance = [").unwrap();
        assert!(save(&path, Appearance::Dark).is_err());
        let directory = root.path().join("blocked");
        fs::create_dir(&directory).unwrap();
        assert!(save(&directory, Appearance::Dark).is_err());
    }

    #[test]
    fn panel_save_reads_atomically_replaced_file_and_preserves_new_comments() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "[appearance]\ntheme_mode = 'light'\n").unwrap();
        let replacement = root.path().join("editor.tmp");
        fs::write(
            &replacement,
            "# replaced by editor\n[appearance]\ntheme_mode = 'dark'\n\n[other]\nvalue = 42 # keep\n",
        )
        .unwrap();
        fs::rename(replacement, &path).unwrap();
        save(&path, Appearance::System).unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::System);
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("# replaced by editor"));
        assert!(content.contains("value = 42 # keep"));
    }

    #[test]
    fn locations_are_separate_and_independent_of_document_paths() {
        let dirs = AppPaths::resolve().expect("platform has a home directory");
        assert!(dirs.config().ends_with("config.toml"));
        assert!(dirs.config().is_absolute());
        assert!(dirs.local_data().is_absolute());
        #[cfg(target_os = "macos")]
        {
            assert!(
                dirs.config()
                    .ends_with("Library/Application Support/com.JonGretar.Kver/config.toml")
            );
            assert!(
                dirs.local_data()
                    .ends_with("Library/Application Support/com.JonGretar.Kver")
            );
        }
        #[cfg(target_os = "linux")]
        {
            assert!(dirs.config().ends_with("kver/config.toml"));
            assert!(dirs.local_data().ends_with("kver"));
        }
        #[cfg(target_os = "windows")]
        {
            assert!(dirs.config().ends_with("JonGretar/Kver/config/config.toml"));
            assert!(dirs.local_data().ends_with("JonGretar/Kver/data"));
        }
        let isolated = AppPaths::for_locations(
            PathBuf::from("/tmp/settings.toml"),
            PathBuf::from("/tmp/data"),
        );
        assert_eq!(isolated.config(), Path::new("/tmp/settings.toml"));
        assert_eq!(isolated.local_data(), Path::new("/tmp/data"));
    }
}
