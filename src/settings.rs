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

/// Default layout for a newly opened document; open readers keep their view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingView {
    #[default]
    Continuous,
    Page,
}

impl ReadingView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Continuous => "Continuous",
            Self::Page => "Page",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Page => "page",
        }
    }

    fn parse(key: &str) -> Result<Self, String> {
        match key {
            "continuous" => Ok(Self::Continuous),
            "page" => Ok(Self::Page),
            _ => Err(format!(
                "Unknown reading view {key:?}; use continuous or page"
            )),
        }
    }
}

/// The complete set of preferences understood by this version of Kver.
/// Unknown TOML entries remain in the file, but are not applied as settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppSettings {
    appearance: Appearance,
    show_toolbar: bool,
    show_statusbar: bool,
    remember_reading_history: bool,
    reopen_at_last_position: bool,
    quit_when_last_window_closes: bool,
    initial_zoom: InitialZoom,
    reopen_last_windows_on_startup: bool,
    reading_view: ReadingView,
}

impl AppSettings {
    pub fn show_toolbar(self) -> bool {
        self.show_toolbar
    }

    pub fn with_show_toolbar(mut self, show: bool) -> Self {
        self.show_toolbar = show;
        self
    }

    pub fn show_statusbar(self) -> bool {
        self.show_statusbar
    }

    pub fn with_show_statusbar(mut self, show: bool) -> Self {
        self.show_statusbar = show;
        self
    }

    pub fn appearance(self) -> Appearance {
        self.appearance
    }

    pub fn with_appearance(mut self, appearance: Appearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn remember_reading_history(self) -> bool {
        self.remember_reading_history
    }

    pub fn with_remember_reading_history(mut self, remember: bool) -> Self {
        self.remember_reading_history = remember;
        self
    }

    pub fn reopen_at_last_position(self) -> bool {
        self.reopen_at_last_position
    }

    pub fn with_reopen_at_last_position(mut self, reopen: bool) -> Self {
        self.reopen_at_last_position = reopen;
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

    pub fn reopen_last_windows_on_startup(self) -> bool {
        self.reopen_last_windows_on_startup
    }

    pub fn with_reopen_last_windows_on_startup(mut self, reopen: bool) -> Self {
        self.reopen_last_windows_on_startup = reopen;
        self
    }

    pub fn reading_view(self) -> ReadingView {
        self.reading_view
    }

    pub fn with_reading_view(mut self, view: ReadingView) -> Self {
        self.reading_view = view;
        self
    }

    fn from_document(document: &DocumentMut) -> Result<Self, String> {
        let application = table_in(document, "application")?;
        let reader = table_in(document, "reader")?;
        Ok(Self {
            appearance: appearance_in(document)?,
            show_toolbar: bool_in(
                application.and_then(|table| table.get("show_toolbar")),
                true,
                "[application].show_toolbar",
            )?,
            show_statusbar: bool_in(
                application.and_then(|table| table.get("show_statusbar")),
                true,
                "[application].show_statusbar",
            )?,
            remember_reading_history: history_in(document)?,
            reopen_at_last_position: bool_in(
                table_in(document, "persistence")?
                    .and_then(|table| table.get("reopen_at_last_position")),
                true,
                "[persistence].reopen_at_last_position",
            )?,
            quit_when_last_window_closes: quit_when_last_window_closes_in(document)?,
            initial_zoom: initial_zoom_in(document)?,
            reopen_last_windows_on_startup: bool_in(
                application.and_then(|table| table.get("reopen_last_windows_on_startup")),
                false,
                "[application].reopen_last_windows_on_startup",
            )?,
            reading_view: match reader.and_then(|table| table.get("reading_view")) {
                None => ReadingView::default(),
                Some(value) => ReadingView::parse(
                    value
                        .as_str()
                        .ok_or("Settings [reader].reading_view must be a string")?,
                )?,
            },
        })
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            show_toolbar: true,
            show_statusbar: true,
            remember_reading_history: true,
            reopen_at_last_position: true,
            quit_when_last_window_closes: true,
            initial_zoom: InitialZoom::default(),
            reopen_last_windows_on_startup: false,
            reading_view: ReadingView::default(),
        }
    }
}

/// An intentional edit to one known preference, not a replacement of the file.
#[derive(Clone, Copy, Debug)]
pub enum SettingsChange {
    Appearance(Appearance),
    ShowToolbar(bool),
    ShowStatusbar(bool),
    RememberReadingHistory(bool),
    ReopenAtLastPosition(bool),
    QuitWhenLastWindowCloses(bool),
    InitialZoom(InitialZoom),
    ReopenLastWindowsOnStartup(bool),
    ReadingView(ReadingView),
}

impl SettingsChange {
    fn apply(self, document: &mut DocumentMut) {
        let (section, key, updated) = match self {
            Self::Appearance(choice) => ("application", "appearance", value(choice.key())),
            Self::ShowToolbar(show) => ("application", "show_toolbar", value(show)),
            Self::ShowStatusbar(show) => ("application", "show_statusbar", value(show)),
            Self::RememberReadingHistory(remember) => ("persistence", "remember", value(remember)),
            Self::ReopenAtLastPosition(reopen) => {
                ("persistence", "reopen_at_last_position", value(reopen))
            }
            Self::QuitWhenLastWindowCloses(quit) => {
                ("application", "quit_when_last_window_closes", value(quit))
            }
            Self::InitialZoom(zoom) => ("reader", "initial_zoom", value(zoom.key())),
            Self::ReopenLastWindowsOnStartup(reopen) => (
                "application",
                "reopen_last_windows_on_startup",
                value(reopen),
            ),
            Self::ReadingView(view) => ("reader", "reading_view", value(view.key())),
        };
        if document.get(section).is_none() {
            document[section] = Item::Table(Table::new());
        }
        let mut updated = updated;
        // Preserve inline comments when replacing a known setting.
        if let Some(existing) = document[section].get(key).and_then(Item::as_value)
            && let Some(value) = updated.as_value_mut()
        {
            *value.decor_mut() = existing.decor().clone();
        }
        document[section][key] = updated;
    }
}

fn table_in<'a>(document: &'a DocumentMut, section: &str) -> Result<Option<&'a Table>, String> {
    document
        .get(section)
        .map(|item| {
            item.as_table()
                .ok_or_else(|| format!("Settings [{section}] must be a table"))
        })
        .transpose()
}

fn bool_in(item: Option<&Item>, default: bool, path: &str) -> Result<bool, String> {
    item.map(|item| {
        item.as_bool()
            .ok_or_else(|| format!("Settings {path} must be a boolean"))
    })
    .unwrap_or(Ok(default))
}

/// Read old chapters until the next panel save moves their known keys.
/// Explicit values in the new chapters take precedence.
fn grouped_item<'a>(
    document: &'a DocumentMut,
    section: &str,
    legacy: &str,
    key: &str,
) -> Result<Option<&'a Item>, String> {
    let current = table_in(document, section)?;
    let old = table_in(document, legacy)?;
    Ok(current
        .and_then(|table| table.get(key))
        .or_else(|| old.and_then(|table| table.get(key))))
}

fn migrate_chapters(document: &mut DocumentMut) {
    for (legacy, section, key) in [
        ("appearance", "application", "theme_mode"),
        ("windows", "application", "quit_when_last_window_closes"),
        ("reading_history", "persistence", "remember"),
    ] {
        let old = document
            .get_mut(legacy)
            .and_then(Item::as_table_mut)
            .and_then(|table| table.remove(key));
        let Some(old) = old else {
            continue;
        };
        if document.get(section).is_none() {
            document[section] = Item::Table(Table::new());
        }
        if document[section].get(key).is_none() {
            document[section][key] = old;
        }
        // Retain legacy tables (and their comments) if they have unknown content.
        if document
            .get(legacy)
            .and_then(Item::as_table)
            .is_some_and(Table::is_empty)
            && let Some(Item::Table(old)) = document.remove(legacy)
            && let Some(table) = document.get_mut(section).and_then(Item::as_table_mut)
        {
            // Keep chapter comments even when the old header disappears.
            let prefix = old
                .decor()
                .prefix()
                .and_then(|text| text.as_str())
                .unwrap_or("");
            let suffix = old
                .decor()
                .suffix()
                .and_then(|text| text.as_str())
                .unwrap_or("")
                .trim();
            let current = table
                .decor()
                .prefix()
                .and_then(|text| text.as_str())
                .unwrap_or("");
            let combined = if suffix.is_empty() {
                format!("{prefix}{current}")
            } else {
                format!("{prefix}{suffix}\n{current}")
            };
            table.decor_mut().set_prefix(combined);
        }
    }
    if let Some(application) = document.get_mut("application").and_then(Item::as_table_mut)
        && let Some(old) = application.remove("theme_mode")
        && !application.contains_key("appearance")
    {
        application.insert("appearance", old);
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
    let legacy = grouped_item(document, "application", "appearance", "theme_mode")?;
    let appearance = table_in(document, "application")?
        .and_then(|table| table.get("appearance"))
        .or(legacy);
    match appearance {
        None => Ok(Appearance::System),
        Some(appearance) => appearance
            .as_str()
            .ok_or_else(|| "Settings [application].appearance must be a string".to_owned())
            .and_then(Appearance::parse),
    }
}

fn history_in(document: &DocumentMut) -> Result<bool, String> {
    bool_in(
        grouped_item(document, "persistence", "reading_history", "remember")?,
        true,
        "[persistence].remember",
    )
}

fn quit_when_last_window_closes_in(document: &DocumentMut) -> Result<bool, String> {
    bool_in(
        grouped_item(
            document,
            "application",
            "windows",
            "quit_when_last_window_closes",
        )?,
        true,
        "[application].quit_when_last_window_closes",
    )
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
    migrate_chapters(&mut document);
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
    fn ui_elements_default_on_and_round_trip_independently() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        assert!(AppSettings::default().show_toolbar());
        assert!(AppSettings::default().show_statusbar());
        let defaults = super::load(&path).unwrap();
        assert!(defaults.show_toolbar());
        assert!(defaults.show_statusbar());
        fs::write(&path, "# keep\n[application]\nappearance = 'dark'\nshow_toolbar = true # toolbar\n[future]\nvalue = 42\n").unwrap();
        let hidden_toolbar = super::save(&path, SettingsChange::ShowToolbar(false)).unwrap();
        assert!(!hidden_toolbar.show_toolbar());
        assert!(hidden_toolbar.show_statusbar());
        let hidden_both = super::save(&path, SettingsChange::ShowStatusbar(false)).unwrap();
        assert_eq!(super::load(&path).unwrap(), hidden_both);
        assert!(!hidden_both.show_toolbar());
        assert!(!hidden_both.show_statusbar());
        assert_eq!(hidden_both.appearance(), Appearance::Dark);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("show_toolbar = false # toolbar"));
        assert!(text.contains("# keep"));
        assert!(text.contains("[future]\nvalue = 42"));
        super::save(&path, SettingsChange::ShowToolbar(true)).unwrap();
        let restored = super::save(&path, SettingsChange::ShowStatusbar(true)).unwrap();
        assert!(restored.show_toolbar());
        assert!(restored.show_statusbar());
        assert_eq!(super::load(&path).unwrap(), restored);
    }

    #[test]
    fn restore_position_choice_is_independent_of_recording_and_preserves_toml() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        assert!(super::load(&path).unwrap().reopen_at_last_position());
        fs::write(&path, "# retain\n[persistence]\nremember = false\nreopen_at_last_position = true # choice\n[other]\nvalue = 7\n").unwrap();
        let updated = super::save(&path, SettingsChange::ReopenAtLastPosition(false)).unwrap();
        assert!(!updated.reopen_at_last_position());
        assert!(!updated.remember_reading_history());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("reopen_at_last_position = false # choice")
        );
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("[other]\nvalue = 7")
        );
        fs::write(&path, "[persistence]\nreopen_at_last_position = 'no'\n").unwrap();
        assert!(super::load(&path).is_err());
        assert!(super::save(&path, SettingsChange::ReopenAtLastPosition(true)).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[persistence]\nreopen_at_last_position = 'no'\n"
        );
    }

    #[test]
    fn panel_chapters_round_trip_and_migrate_legacy_settings() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "# theme chapter\n[appearance]\ntheme_mode = 'dark' # theme\naccent = 'violet'\n\n# windows chapter\n[windows]\nquit_when_last_window_closes = false # close\n\n# history chapter\n[reading_history]\nremember = false # privacy\n").unwrap();
        let before = super::load(&path).unwrap();
        assert!(!before.reopen_last_windows_on_startup());
        assert_eq!(before.reading_view(), ReadingView::Continuous);
        super::save(&path, SettingsChange::ReopenLastWindowsOnStartup(true)).unwrap();
        let updated = super::save(&path, SettingsChange::ReadingView(ReadingView::Page)).unwrap();
        assert_eq!(
            updated,
            before
                .with_reopen_last_windows_on_startup(true)
                .with_reading_view(ReadingView::Page)
        );
        assert_eq!(super::load(&path).unwrap(), updated);
        let text = fs::read_to_string(&path).unwrap();
        let document: DocumentMut = text.parse().unwrap();
        assert_eq!(document["application"]["appearance"].as_str(), Some("dark"));
        assert_eq!(
            document["application"]["quit_when_last_window_closes"].as_bool(),
            Some(false)
        );
        assert_eq!(document["persistence"]["remember"].as_bool(), Some(false));
        assert_eq!(document["reader"]["reading_view"].as_str(), Some("page"));
        assert!(document.get("windows").is_none());
        assert!(document.get("reading_history").is_none());
        assert_eq!(document["appearance"]["accent"].as_str(), Some("violet"));
        for comment in [
            "# theme chapter",
            "# windows chapter",
            "# history chapter",
            "# theme",
            "# close",
            "# privacy",
        ] {
            assert!(text.contains(comment), "lost {comment}");
        }
        super::save(&path, SettingsChange::ReadingView(ReadingView::Continuous)).unwrap();
        super::save(&path, SettingsChange::ReopenLastWindowsOnStartup(false)).unwrap();
        assert_eq!(super::load(&path).unwrap(), before);
    }

    #[test]
    fn new_chapters_take_precedence_and_reject_invalid_values_without_writing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "[appearance]\ntheme_mode = 'light'\n[application]\ntheme_mode = 'dark'\n[persistence]\nremember = false\n[reading_history]\nremember = true\n").unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::Dark);
        assert!(!super::load(&path).unwrap().remember_reading_history());
        save(&path, Appearance::System).unwrap();
        assert!(!super::load(&path).unwrap().remember_reading_history());
        for text in [
            "application = false\n",
            "persistence = false\n",
            "[application]\nappearance = 'purple'\n",
            "[application]\nappearance = false\n",
            "[application]\ntheme_mode = 'purple'\n",
            "[application]\nquit_when_last_window_closes = 'yes'\n",
            "[application]\nreopen_last_windows_on_startup = 'yes'\n",
            "[application]\nshow_toolbar = 'yes'\n",
            "[application]\nshow_statusbar = 1\n",
            "[persistence]\nremember = 'yes'\n",
            "[reader]\nreading_view = 'other'\n",
            "[reader]\nreading_view = true\n",
        ] {
            fs::write(&path, text).unwrap();
            assert!(super::load(&path).is_err(), "{text}");
            assert!(save(&path, Appearance::Dark).is_err(), "{text}");
            assert_eq!(fs::read_to_string(&path).unwrap(), text);
        }
    }

    #[test]
    fn appearance_key_renames_theme_mode_and_preserves_comments() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        for section in ["application", "appearance"] {
            fs::write(
                &path,
                format!("# keep chapter\n[{section}]\ntheme_mode = 'dark' # keep choice\n"),
            )
            .unwrap();
            assert_eq!(load(&path).unwrap(), Appearance::Dark);
            super::save(&path, SettingsChange::InitialZoom(InitialZoom::FitWidth)).unwrap();
            let text = fs::read_to_string(&path).unwrap();
            let document: DocumentMut = text.parse().unwrap();
            assert_eq!(document["application"]["appearance"].as_str(), Some("dark"));
            assert!(document["application"].get("theme_mode").is_none());
            assert!(!text.contains("theme_mode"));
            assert!(text.contains("# keep chapter"));
            assert!(text.contains("# keep choice"));
            save(&path, Appearance::Light).unwrap();
            assert_eq!(load(&path).unwrap(), Appearance::Light);
        }

        fs::write(&path, "[application]\nappearance = 'light'\ntheme_mode = 'dark'\n[appearance]\ntheme_mode = 'system'\n").unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::Light);
        super::save(&path, SettingsChange::InitialZoom(InitialZoom::FitPage)).unwrap();
        assert_eq!(load(&path).unwrap(), Appearance::Light);
        assert!(!fs::read_to_string(&path).unwrap().contains("theme_mode"));
    }

    #[test]
    fn missing_file_defaults_without_creating_directories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/config.toml");
        assert_eq!(load(&path).unwrap(), Appearance::System);
        assert!(super::load(&path).unwrap().remember_reading_history());
        assert!(!path.exists());
    }

    #[test]
    fn history_choice_preserves_other_toml_and_rejects_invalid_edits() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            "# keep\n[reading_history]\nremember = true # keep inline\n[other]\nvalue = 7\n",
        )
        .unwrap();
        super::save(&path, SettingsChange::RememberReadingHistory(false)).unwrap();
        assert!(!super::load(&path).unwrap().remember_reading_history());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("remember = false # keep inline"));
        assert!(text.contains("[other]\nvalue = 7"));
        fs::write(&path, "[reading_history]\nremember = 'no'\n").unwrap();
        assert!(super::load(&path).is_err());
        assert!(super::save(&path, SettingsChange::RememberReadingHistory(true)).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[reading_history]\nremember = 'no'\n"
        );
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
            "[application]\nappearance = \"light\"\n"
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
