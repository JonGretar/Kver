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

/// Default view for a newly opened document, not a live reader command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InitialZoom {
    #[default]
    ActualSize,
    FitWidth,
    FitPage,
}

impl InitialZoom {
    pub fn label(self) -> &'static str {
        match self {
            Self::ActualSize => "100%",
            Self::FitWidth => "Fit Width",
            Self::FitPage => "Fit Page",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::ActualSize => "100_percent",
            Self::FitWidth => "fit_width",
            Self::FitPage => "fit_page",
        }
    }

    fn parse(key: &str) -> Result<Self, String> {
        match key {
            "100_percent" => Ok(Self::ActualSize),
            "fit_width" => Ok(Self::FitWidth),
            "fit_page" => Ok(Self::FitPage),
            _ => Err(format!(
                "Unknown initial zoom {key:?}; use 100_percent, fit_width, or fit_page"
            )),
        }
    }
}

/// The complete set of preferences understood by this version of Kver.
/// Unknown TOML entries remain in the file, but are not applied as settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppSettings {
    appearance: Appearance,
    quit_when_last_window_closes: bool,
    initial_zoom: InitialZoom,
}

impl AppSettings {
    pub fn appearance(self) -> Appearance {
        self.appearance
    }

    pub fn with_appearance(mut self, appearance: Appearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn quit_when_last_window_closes(self) -> bool {
        self.quit_when_last_window_closes
    }

    pub fn with_quit_when_last_window_closes(mut self, quit: bool) -> Self {
        self.quit_when_last_window_closes = quit;
        self
    }

    pub fn initial_zoom(self) -> InitialZoom {
        self.initial_zoom
    }

    pub fn with_initial_zoom(mut self, initial_zoom: InitialZoom) -> Self {
        self.initial_zoom = initial_zoom;
        self
    }

    fn from_document(document: &DocumentMut) -> Result<Self, String> {
        Ok(Self {
            appearance: appearance_in(document)?,
            quit_when_last_window_closes: quit_when_last_window_closes_in(document)?,
            initial_zoom: initial_zoom_in(document)?,
        })
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            quit_when_last_window_closes: true,
            initial_zoom: InitialZoom::default(),
        }
    }
}

/// An intentional edit to one known preference, not a replacement of the file.
#[derive(Clone, Copy, Debug)]
pub enum SettingsChange {
    Appearance(Appearance),
    QuitWhenLastWindowCloses(bool),
    InitialZoom(InitialZoom),
}

impl SettingsChange {
    fn apply(self, document: &mut DocumentMut) {
        match self {
            Self::Appearance(appearance) => {
                if document.get("appearance").is_none() {
                    document["appearance"] = Item::Table(Table::new());
                }
                if let Some(existing) = document["appearance"]["theme_mode"].as_value_mut() {
                    // Replacing an Item loses its inline comment; keep its decor.
                    let decor = existing.decor().clone();
                    *existing = appearance.key().into();
                    *existing.decor_mut() = decor;
                } else {
                    document["appearance"]["theme_mode"] = value(appearance.key());
                }
            }
            Self::QuitWhenLastWindowCloses(quit) => {
                if document.get("windows").is_none() {
                    document["windows"] = Item::Table(Table::new());
                }
                if let Some(existing) =
                    document["windows"]["quit_when_last_window_closes"].as_value_mut()
                {
                    let decor = existing.decor().clone();
                    *existing = quit.into();
                    *existing.decor_mut() = decor;
                } else {
                    document["windows"]["quit_when_last_window_closes"] = value(quit);
                }
            }
            Self::InitialZoom(zoom) => {
                if document.get("reader").is_none() {
                    document["reader"] = Item::Table(Table::new());
                }
                if let Some(existing) = document["reader"]["initial_zoom"].as_value_mut() {
                    let decor = existing.decor().clone();
                    *existing = zoom.key().into();
                    *existing.decor_mut() = decor;
                } else {
                    document["reader"]["initial_zoom"] = value(zoom.key());
                }
            }
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

fn quit_when_last_window_closes_in(document: &DocumentMut) -> Result<bool, String> {
    let Some(windows) = document.get("windows") else {
        return Ok(true);
    };
    let windows = windows
        .as_table()
        .ok_or("Settings [windows] must be a table")?;
    match windows.get("quit_when_last_window_closes") {
        None => Ok(true),
        Some(quit) => quit.as_bool().ok_or_else(|| {
            "Settings [windows].quit_when_last_window_closes must be a boolean".to_owned()
        }),
    }
}

fn initial_zoom_in(document: &DocumentMut) -> Result<InitialZoom, String> {
    let Some(reader) = document.get("reader") else {
        return Ok(InitialZoom::ActualSize);
    };
    let reader = reader
        .as_table()
        .ok_or("Settings [reader] must be a table")?;
    match reader.get("initial_zoom") {
        None => Ok(InitialZoom::ActualSize),
        Some(value) => value
            .as_str()
            .ok_or_else(|| "Settings [reader].initial_zoom must be a string".to_owned())
            .and_then(InitialZoom::parse),
    }
}

pub fn load(path: &Path) -> Result<AppSettings, String> {
    AppSettings::from_document(&read_document(path)?)
}

/// Re-read before writing so unrelated hand-edited TOML remains intact.
/// Refuse invalid input rather than replacing a file the user needs to repair.
pub fn save(path: &Path, change: SettingsChange) -> Result<AppSettings, String> {
    let original = read_content(path)?;
    let mut document = parse_document(path, original.as_deref())?;
    AppSettings::from_document(&document)?;
    change.apply(&mut document);
    let updated = AppSettings::from_document(&document)?;
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
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save(path: &Path, appearance: Appearance) -> Result<AppSettings, String> {
        super::save(path, SettingsChange::Appearance(appearance))
    }

    fn load(path: &Path) -> Result<Appearance, String> {
        super::load(path).map(AppSettings::appearance)
    }

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
    fn last_window_choice_defaults_on_and_preserves_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        assert!(super::load(&path).unwrap().quit_when_last_window_closes());
        fs::write(
            &path,
            "# keep\n[appearance]\ntheme_mode = 'dark'\n[windows]\nquit_when_last_window_closes = true # keep inline\n[future]\nvalue = 42\n",
        )
        .unwrap();
        super::save(&path, SettingsChange::QuitWhenLastWindowCloses(false)).unwrap();
        let restarted = super::load(&path).unwrap();
        assert!(!restarted.quit_when_last_window_closes());
        assert_eq!(restarted.appearance(), Appearance::Dark);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# keep inline"));
        assert!(content.contains("[future]\nvalue = 42"));
        assert!(content.contains("quit_when_last_window_closes = false"));
        super::save(&path, SettingsChange::Appearance(Appearance::Light)).unwrap();
        assert!(!super::load(&path).unwrap().quit_when_last_window_closes());
    }

    #[test]
    fn invalid_last_window_choice_does_not_overwrite_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for content in [
            "windows = false\n",
            "[windows]\nquit_when_last_window_closes = 'no'\n",
        ] {
            fs::write(&path, content).unwrap();
            assert!(super::load(&path).is_err());
            assert!(super::save(&path, SettingsChange::QuitWhenLastWindowCloses(false)).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }

    #[test]
    fn initial_zoom_round_trips_without_disturbing_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        assert_eq!(
            super::load(&path).unwrap().initial_zoom(),
            InitialZoom::ActualSize
        );
        fs::write(
            &path,
            "# keep\n[appearance]\ntheme_mode = 'dark'\n[reader]\ninitial_zoom = 'fit_width' # keep inline\nother = 42\n",
        )
        .unwrap();
        assert_eq!(
            super::load(&path).unwrap().initial_zoom(),
            InitialZoom::FitWidth
        );
        let updated =
            super::save(&path, SettingsChange::InitialZoom(InitialZoom::FitPage)).unwrap();
        assert_eq!(updated.initial_zoom(), InitialZoom::FitPage);
        assert_eq!(updated.appearance(), Appearance::Dark);
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# keep\n"));
        assert!(content.contains(" # keep inline"));
        assert!(content.contains("other = 42"));
        assert_eq!(super::load(&path).unwrap(), updated);

        fs::write(&path, "[appearance]\ntheme_mode = 'light'\n").unwrap();
        super::save(&path, SettingsChange::InitialZoom(InitialZoom::ActualSize)).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("[reader]\ninitial_zoom = \"100_percent\"")
        );
        assert_eq!(super::load(&path).unwrap().appearance(), Appearance::Light);
    }

    #[test]
    fn invalid_initial_zoom_keeps_the_file_untouched() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for content in [
            "[reader]\ninitial_zoom = 'other'\n",
            "[reader]\ninitial_zoom = 10\n",
            "reader = 'fit_width'\n",
        ] {
            fs::write(&path, content).unwrap();
            assert!(super::load(&path).is_err());
            assert!(super::save(&path, SettingsChange::Appearance(Appearance::Dark)).is_err());
            assert!(
                super::save(&path, SettingsChange::InitialZoom(InitialZoom::FitWidth)).is_err()
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
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
