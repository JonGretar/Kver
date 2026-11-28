//! GitHub announces stable releases; only the official installer and a pinned
//! signing key authorize replacement. The ribbon itself knows neither policy.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))] // Exercise Linux policy on Mac CI.
use super::{
    commands,
    ribbon::{self, Choice, Notice},
};
use crate::settings::{self, AppPaths};
use gpui_kit::{App, AppContext as _, Global};
use semver::Version;
use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const KEY: Option<&str> = option_env!("KVER_OFFICIAL_LINUX_UPDATE_KEY");
const HELPER: &str = include_str!("../../packaging/linux/update.py");
const DAY: Duration = Duration::from_secs(86400);

fn helper(key: &str, action: &str, executable: &Path) -> Command {
    let mut cmd = Command::new("/usr/bin/python3");
    cmd.args(["-I", "-c", HELPER, action])
        .arg(executable)
        .arg(key);
    cmd
}

#[derive(Clone)]
struct Release {
    version: Version,
    signed_asset: bool,
}

impl Release {
    fn newer(json: &Value, current: &Version) -> Result<Option<Self>, String> {
        let version = json["tag_name"]
            .as_str()
            .and_then(|s| s.strip_prefix('v'))
            .and_then(|s| Version::parse(s).ok())
            .ok_or("Invalid Kver release tag")?;
        if json["draft"] != false
            || json["prerelease"] != false
            || !version.pre.is_empty()
            || !version.build.is_empty()
            || version.cmp_precedence(current).is_le()
        {
            return Ok(None);
        }
        let archive = format!("Kver-v{version}-linux-x86_64.tar.gz");
        let assets = json["assets"].as_array().ok_or("Missing release assets")?;
        let signed_asset = ["", ".sha256", ".sha256.sig"].iter().all(|suffix| {
            assets
                .iter()
                .any(|asset| asset["name"] == format!("{archive}{suffix}"))
        });
        Ok(Some(Self {
            version,
            signed_asset,
        }))
    }
    fn url(&self) -> String {
        format!("{}/releases/tag/v{}", commands::PROJECT_URL, self.version)
    }
}

#[derive(Clone)]
struct Offer {
    release: Release,
    managed: Option<Managed>,
}

#[derive(Clone)]
struct Managed {
    exe: PathBuf,
    identity: String,
    key: &'static str,
}

impl Managed {
    fn detect() -> Option<Self> {
        let key = KEY.filter(|s| !s.is_empty())?;
        let exe = std::env::current_exe().ok()?;
        let home = PathBuf::from(std::env::var_os("HOME")?);
        if exe != home.join(".local/opt/kver/kver") {
            return None;
        }
        // Give the helper this process's actual executable, not a path that
        // might now refer to a replaced file or require cross-process /proc access.
        let running = fs::File::open("/proc/self/exe").ok()?;
        let output = helper(key, "eligible", &exe).stdin(running).output().ok()?;
        let identity = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (output.status.success() && identity.len() == 64).then_some(Self { exe, identity, key })
    }
    fn stage(&self, version: &Version) -> Result<tempfile::TempDir, String> {
        let stage = tempfile::Builder::new()
            .prefix("kver-update-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(
                std::env::temp_dir()
                    .canonicalize()
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        let output = helper(self.key, "stage", &self.exe)
            .arg(version.to_string())
            .arg(stage.path())
            .arg(&self.identity)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(stage)
    }
}

fn discover() -> Result<Option<Offer>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .build()
        .into();
    let body = agent
        .get("https://api.github.com/repos/JonGretar/Kver/releases/latest")
        .header("User-Agent", concat!("Kver/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    let json = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let release = Release::newer(
        &json,
        &Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?,
    )?;
    Ok(release.map(|release| Offer {
        managed: release.signed_asset.then(Managed::detect).flatten(),
        release,
    }))
}

enum Phase {
    Idle,
    Checking,
    Available(Offer),
    Downloading(Version),
    Ready(Managed, Version, tempfile::TempDir),
}
struct Updates {
    phase: Phase,
    hidden: bool,
    manual: bool,
    restart_requested: bool,
}
impl Global for Updates {}

fn state_path(name: &str) -> Option<PathBuf> {
    Some(AppPaths::resolve()?.local_data().join(name))
}
fn store(path: &Path, value: &str) -> std::io::Result<()> {
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(value.as_bytes())?;
    temp.persist(path)?;
    Ok(())
}

fn automatic_due() -> bool {
    let Some(paths) = AppPaths::resolve() else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    claim_check(&paths, now)
}

fn claim_check(paths: &AppPaths, now: u64) -> bool {
    if !settings::load(paths.config()).is_ok_and(|s| s.automatic_update_checks()) {
        return false;
    }
    let file = paths.local_data().join("last-update-check");
    let last = fs::read_to_string(&file)
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    if last.is_some_and(|v| v <= now && now - v < DAY.as_secs()) {
        return false;
    }
    store(&file, &now.to_string()).is_ok()
}

pub(super) fn install(cx: &mut App) {
    cx.set_global(Updates {
        phase: Phase::Idle,
        hidden: false,
        manual: false,
        restart_requested: false,
    });
    commands::register_update_check(cx, |cx| check(true, cx));
    if let Ok(reason) = std::env::var("KVER_UPDATE_FAILURE") {
        info(format!("Kver could not install the update: {reason}"), cx);
    }
    cx.spawn(async move |cx| {
        loop {
            if cx.background_spawn(async { automatic_due() }).await {
                cx.update(|cx| check(false, cx));
            }
            cx.background_executor().timer(DAY).await;
        }
    })
    .detach();
}

fn info(message: String, cx: &mut App) {
    ribbon::present(
        Some(Notice {
            message,
            action: None,
            link: None,
            respond: Rc::new(|_, _| {}),
        }),
        cx,
    );
}

fn check(manual: bool, cx: &mut App) {
    let state = cx.global_mut::<Updates>();
    if !matches!(state.phase, Phase::Idle | Phase::Available(_)) {
        if manual {
            state.manual = true;
            state.hidden = false;
            show(cx);
        }
        return;
    }
    let replacing_offer = matches!(state.phase, Phase::Available(_));
    state.phase = Phase::Checking;
    state.hidden = false;
    state.manual = manual;
    if manual {
        show(cx);
    } else if replacing_offer {
        // The old Download button must not remain clickable while the
        // candidate and its installation eligibility are being refreshed.
        ribbon::present(None, cx);
    }
    cx.spawn(async move |cx| {
        let (result, dismissed) = cx
            .background_spawn(async {
                (
                    discover(),
                    state_path("dismissed-update").and_then(|p| fs::read_to_string(p).ok()),
                )
            })
            .await;
        cx.update(|cx| {
            let state = cx.global_mut::<Updates>();
            state.phase = Phase::Idle;
            match result {
                Ok(Some(offer)) => {
                    state.hidden |=
                        !state.manual && dismissed == Some(offer.release.version.to_string());
                    state.phase = Phase::Available(offer);
                    show(cx);
                }
                result if state.manual && !state.hidden => info(
                    match result {
                        Err(e) => format!("Could not check for updates: {e}"),
                        _ => "Kver is up to date.".into(),
                    },
                    cx,
                ),
                _ => {}
            }
        });
    })
    .detach();
}

fn show(cx: &mut App) {
    let state = cx.global::<Updates>();
    if state.hidden {
        return;
    }
    let view = match &state.phase {
        Phase::Idle => return,
        Phase::Checking => UpdateNotice::Checking,
        Phase::Downloading(_) => UpdateNotice::Downloading,
        Phase::Available(offer) => UpdateNotice::Available(&offer.release, offer.managed.is_some()),
        Phase::Ready(_, version, _) => UpdateNotice::Ready(version),
    };
    ribbon::present(Some(view.notice(Rc::new(respond))), cx);
}

enum UpdateNotice<'a> {
    Checking,
    Downloading,
    Available(&'a Release, bool),
    Ready(&'a Version),
}

impl UpdateNotice<'_> {
    fn notice(self, respond: ribbon::Response) -> Notice {
        let (message, label, link) = match self {
            Self::Checking => ("Checking for Kver updates…".into(), None, None),
            Self::Downloading => ("Downloading and verifying Kver update…".into(), None, None),
            Self::Available(release, managed) => {
                let label = if managed {
                    "Release notes"
                } else {
                    "View release"
                };
                (
                    format!("Kver {} is available.", release.version),
                    managed.then_some("Download update"),
                    Some((label.into(), release.url())),
                )
            }
            Self::Ready(version) => (
                format!("Kver {version} is ready to update."),
                Some("Restart to update"),
                None,
            ),
        };
        Notice {
            message,
            action: label.map(str::to_owned),
            link,
            respond,
        }
    }
}

/// Preview responses never reach the real update engine or restart path.
#[cfg(feature = "screenshot")]
pub fn preview_ribbon(managed: bool, cx: &mut App) {
    let release = Release {
        version: Version::new(0, 0, 2),
        signed_asset: true,
    };
    let version = release.version.clone();
    let notice = UpdateNotice::Available(&release, managed).notice(Rc::new(move |choice, cx| {
        if choice == Choice::Dismiss {
            return;
        }
        let version = version.clone();
        let hidden = Rc::new(std::cell::Cell::new(false));
        let dismiss = hidden.clone();
        let progress = UpdateNotice::Downloading.notice(Rc::new(move |_, _| dismiss.set(true)));
        ribbon::present(Some(progress), cx);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if !hidden.get() {
                cx.update(|cx| {
                    let ready = UpdateNotice::Ready(&version).notice(Rc::new(|choice, cx| {
                        if choice == Choice::Activate {
                            info("Preview complete. No update was installed.".into(), cx);
                        }
                    }));
                    ribbon::present(Some(ready), cx);
                });
            }
        })
        .detach();
    }));
    ribbon::present(Some(notice), cx);
}

fn respond(choice: Choice, cx: &mut App) {
    let state = cx.global_mut::<Updates>();
    if choice == Choice::Dismiss {
        state.hidden = true;
        let version = match &state.phase {
            Phase::Available(offer) => Some(&offer.release.version),
            Phase::Downloading(v) | Phase::Ready(_, v, _) => Some(v),
            _ => None,
        };
        if let Some(version) = version.map(ToString::to_string) {
            cx.background_spawn(async move {
                if let Some(path) = state_path("dismissed-update") {
                    let _ = store(&path, &version);
                }
            })
            .detach();
        }
        return;
    }
    match &state.phase {
        Phase::Available(offer) => {
            let Some(managed) = offer.managed.clone() else {
                return;
            };
            let release = offer.release.clone();
            state.phase = Phase::Downloading(release.version.clone());
            show(cx);
            cx.spawn(async move |cx| {
                let (managed, release, staged) = cx
                    .background_spawn(async move {
                        let staged = managed.stage(&release.version);
                        (managed, release, staged)
                    })
                    .await;
                cx.update(|cx| match staged {
                    Ok(stage) => {
                        cx.global_mut::<Updates>().phase =
                            Phase::Ready(managed, release.version, stage);
                        show(cx);
                    }
                    Err(error) => {
                        cx.global_mut::<Updates>().phase = Phase::Idle;
                        ribbon::present(
                            Some(Notice {
                                message: format!("Could not prepare update: {error}"),
                                action: None,
                                link: Some(("View release".into(), release.url())),
                                respond: Rc::new(|_, _| {}),
                            }),
                            cx,
                        );
                    }
                });
            })
            .detach();
        }
        Phase::Ready(..) => {
            state.restart_requested = true;
            super::windows::request_quit(cx);
        }
        _ => {}
    }
}

pub(super) fn cancel_restart(cx: &mut App) {
    if cx
        .try_global::<Updates>()
        .is_some_and(|s| s.restart_requested)
    {
        cx.global_mut::<Updates>().restart_requested = false;
        show(cx);
    }
}

pub(super) fn approve_restart(cx: &mut App) -> bool {
    if !cx
        .try_global::<Updates>()
        .is_some_and(|s| s.restart_requested)
    {
        return true;
    }
    let state = cx.global_mut::<Updates>();
    state.restart_requested = false;
    let Phase::Ready(managed, version, stage) = std::mem::replace(&mut state.phase, Phase::Idle)
    else {
        return false;
    };
    let Some(lock) = state_path("instance.lock") else {
        info("Cannot locate the Kver installation lock.".into(), cx);
        return false;
    };
    match helper(managed.key, "restart", &managed.exe)
        .arg(version.to_string())
        .arg(stage.path())
        .arg(std::process::id().to_string())
        .arg(managed.identity)
        .arg(lock)
        .spawn()
    {
        Ok(_) => {
            let _ = stage.keep();
            true
        }
        Err(e) => {
            info(format!("Could not start update installer: {e}"), cx);
            false
        }
    }
}

#[cfg(test)]
pub(crate) fn test_notice(managed: bool, ready: bool, cx: &mut App) {
    let version = Version::new(1, 2, 3);
    let install = Managed {
        exe: "/not-an-installed-kver".into(),
        identity: String::new(),
        key: "",
    };
    let phase = if ready {
        Phase::Ready(install, version, tempfile::tempdir().unwrap())
    } else {
        Phase::Available(Offer {
            release: Release {
                version,
                signed_asset: true,
            },
            managed: managed.then_some(install),
        })
    };
    cx.set_global(Updates {
        phase,
        hidden: false,
        manual: true,
        restart_requested: false,
    });
    show(cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linux_update_checks_respect_opt_out_and_survive_restart() {
        let home = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(home.path().join("config.toml"), home.path().join("data"));
        fs::write(
            paths.config(),
            "[application]\nautomatic_update_checks = false\n",
        )
        .unwrap();
        assert!(!claim_check(&paths, 100));
        assert!(!paths.local_data().exists());
        fs::write(
            paths.config(),
            "[application]\nautomatic_update_checks = true\n",
        )
        .unwrap();
        assert!(claim_check(&paths, 100));
        assert!(!claim_check(&paths, 101));
        assert!(claim_check(&paths, 100 + DAY.as_secs()));
    }

    #[test]
    fn stable_release_is_visible_before_signed_linux_assets_exist() {
        let mut json =
            serde_json::json!({"tag_name":"v0.10.0","prerelease":false,"draft":false,"assets":[]});
        let current = Version::new(0, 9, 0);
        assert!(
            !Release::newer(&json, &current)
                .unwrap()
                .unwrap()
                .signed_asset
        );
        for suffix in ["", ".sha256", ".sha256.sig"] {
            json["assets"].as_array_mut().unwrap().push(
                serde_json::json!({"name":format!("Kver-v0.10.0-linux-x86_64.tar.gz{suffix}")}),
            );
        }
        assert!(
            Release::newer(&json, &current)
                .unwrap()
                .unwrap()
                .signed_asset
        );
        assert!(
            Release::newer(&json, &Version::new(0, 10, 0))
                .unwrap()
                .is_none()
        );
        json["prerelease"] = true.into();
        assert!(Release::newer(&json, &current).unwrap().is_none());
    }
}
