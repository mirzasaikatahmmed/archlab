//! Uninstalling ArchLab from inside the app.
//!
//! The app only offers to uninstall a copy that an installer put in place: a macOS bundle in
//! `Applications`, a Windows install with its `uninstall.exe`, a Linux AppImage, or a Linux package
//! it can identify. Anything else (a development build, a copy that was just unpacked somewhere)
//! is refused, so the button can never delete a source checkout or an unrelated folder.
//!
//! What to do is decided by pure functions from a [`Context`], so every platform's choice is
//! tested on any computer; only [`detect`] and [`execute`] touch the system.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use specta::Type;

use crate::runtime::RuntimeError;

/// How a copy of the app was installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// macOS: move the `.app` bundle to the Trash.
    MacTrash { bundle: PathBuf },
    /// Windows: run the installer's own uninstaller.
    WindowsUninstaller { uninstaller: PathBuf },
    /// Linux: delete the AppImage file.
    AppImage { file: PathBuf },
    /// Linux: remove the installed package (asks for the administrator password).
    Package { name: String },
    /// Nothing safe to do.
    Unsupported { reason: String },
}

/// Facts about this copy of the app, gathered by [`detect`].
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub os: &'static str,
    pub exe: PathBuf,
    pub home: Option<PathBuf>,
    /// Windows: the uninstaller next to the program, if there is one.
    pub uninstaller: Option<PathBuf>,
    /// Linux: the AppImage file this process was started from.
    pub appimage: Option<PathBuf>,
    /// Linux: the package that owns the program file.
    pub package: Option<String>,
}

/// What the person is shown before they confirm.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPlan {
    pub supported: bool,
    /// One line: what will happen.
    pub title: String,
    pub details: Vec<String>,
    pub asks_for_password: bool,
    /// Whether the app can delete the person's projects and settings as part of this.
    pub can_remove_data: bool,
    pub data_dir: String,
}

fn in_applications(bundle: &Path, home: Option<&Path>) -> bool {
    let parent = bundle.parent();
    parent == Some(Path::new("/Applications"))
        || home.is_some_and(|h| parent == Some(h.join("Applications").as_path()))
}

/// Chooses what to do for this copy of the app. Pure.
pub fn choose(ctx: &Context) -> Action {
    let refuse = |reason: &str| Action::Unsupported {
        reason: reason.to_owned(),
    };
    match ctx.os {
        "macos" => {
            // .../ArchLab.app/Contents/MacOS/archlab
            let bundle = ctx.exe.ancestors().nth(3).map(Path::to_path_buf);
            match bundle {
                Some(b)
                    if b.extension().is_some_and(|e| e == "app")
                        && in_applications(&b, ctx.home.as_deref()) =>
                {
                    Action::MacTrash { bundle: b }
                }
                _ => refuse(
                    "ArchLab is not installed in your Applications folder, so there is nothing to uninstall here.",
                ),
            }
        }
        "windows" => match &ctx.uninstaller {
            Some(u) => Action::WindowsUninstaller {
                uninstaller: u.clone(),
            },
            None => refuse(
                "This copy was not installed with the installer. Use Windows Settings, Apps, to remove it.",
            ),
        },
        "linux" => {
            if let Some(file) = &ctx.appimage {
                Action::AppImage { file: file.clone() }
            } else if let Some(name) = &ctx.package {
                Action::Package { name: name.clone() }
            } else {
                refuse(
                    "This copy was not installed from a package or run as an AppImage, so there is nothing to uninstall here.",
                )
            }
        }
        _ => refuse("Uninstalling from inside the app is not supported on this system."),
    }
}

/// The text for the confirmation. Pure.
pub fn describe(action: &Action, data_dir: &Path) -> UninstallPlan {
    let data = data_dir.display().to_string();
    let (supported, title, details, asks_for_password, can_remove_data) = match action {
        Action::MacTrash { bundle } => (
            true,
            "Move ArchLab to the Trash".to_owned(),
            vec![
                format!(
                    "{} is moved to the Trash, so you can still restore it.",
                    bundle.display()
                ),
                "ArchLab quits first.".to_owned(),
            ],
            false,
            true,
        ),
        Action::WindowsUninstaller { .. } => (
            true,
            "Start the ArchLab uninstaller".to_owned(),
            vec![
                "ArchLab quits and the uninstaller opens.".to_owned(),
                "The uninstaller asks whether to delete your projects and settings.".to_owned(),
            ],
            false,
            false,
        ),
        Action::AppImage { file } => (
            true,
            "Delete the ArchLab AppImage".to_owned(),
            vec![
                format!("{} is deleted after ArchLab quits.", file.display()),
                "Menu entries you added by hand are not touched.".to_owned(),
            ],
            false,
            true,
        ),
        Action::Package { name } => (
            true,
            format!("Remove the package \"{name}\""),
            vec![
                "Your system asks for the administrator password.".to_owned(),
                "ArchLab quits when the package has been removed.".to_owned(),
            ],
            true,
            true,
        ),
        Action::Unsupported { reason } => (false, reason.clone(), Vec::new(), false, false),
    };
    UninstallPlan {
        supported,
        title,
        details,
        asks_for_password,
        can_remove_data,
        data_dir: data,
    }
}

/// Gathers the facts about this copy of the app.
pub fn detect() -> Context {
    let exe = std::env::current_exe().unwrap_or_default();
    let os = std::env::consts::OS;
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let uninstaller = exe
        .parent()
        .map(|d| d.join("uninstall.exe"))
        .filter(|p| p.is_file());
    let appimage = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    let package = if os == "linux" && appimage.is_none() {
        owning_package(&exe)
    } else {
        None
    };
    Context {
        os: match os {
            "macos" => "macos",
            "windows" => "windows",
            "linux" => "linux",
            _ => "other",
        },
        exe,
        home,
        uninstaller,
        appimage,
        package,
    }
}

/// The Debian package that owns `file`, if `dpkg` knows it.
fn owning_package(file: &Path) -> Option<String> {
    let out = Command::new("dpkg").arg("-S").arg(file).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let name = text.lines().next()?.split(':').next()?.trim();
    (!name.is_empty() && !name.contains(' ')).then(|| name.to_owned())
}

/// Arguments for the shell helper that runs after the app has quit: `sh -c SCRIPT sh ARGS...`.
/// Paths travel as arguments, never inside the script text.
pub fn helper_command(action: &Action, trash: &Path, delay_s: u32) -> Option<Vec<String>> {
    let script = match action {
        Action::MacTrash { .. } => format!("sleep {delay_s}; mv \"$1\" \"$2\""),
        Action::AppImage { .. } => format!("sleep {delay_s}; rm -f \"$1\""),
        _ => return None,
    };
    let target = match action {
        Action::MacTrash { bundle } => vec![
            bundle.display().to_string(),
            trash
                .join(
                    bundle
                        .file_name()
                        .map_or_else(|| "ArchLab.app".into(), |n| n.to_os_string()),
                )
                .display()
                .to_string(),
        ],
        Action::AppImage { file } => vec![file.display().to_string()],
        _ => return None,
    };
    let mut args = vec!["-c".to_owned(), script, "sh".to_owned()];
    args.extend(target);
    Some(args)
}

fn spawn_helper(args: &[String]) -> Result<(), RuntimeError> {
    Command::new("sh")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| RuntimeError::Failed(format!("could not start the uninstall helper: {e}")))
}

/// Deletes the projects and settings. Called only when the person asked for it.
fn remove_data(data_dir: &Path) -> Result<(), RuntimeError> {
    if data_dir.as_os_str().is_empty() || !data_dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(data_dir)
        .map_err(|e| RuntimeError::Failed(format!("could not delete {}: {e}", data_dir.display())))
}

/// Runs the uninstall for `action`. The caller quits the app when this returns `Ok`.
pub fn execute(
    action: &Action,
    delete_data: bool,
    data_dir: &Path,
    home: Option<&Path>,
) -> Result<(), RuntimeError> {
    match action {
        Action::Unsupported { reason } => Err(RuntimeError::Failed(reason.clone())),
        Action::WindowsUninstaller { uninstaller } => Command::new(uninstaller)
            .spawn()
            .map(|_| ())
            .map_err(|e| RuntimeError::Failed(format!("could not start the uninstaller: {e}"))),
        Action::Package { name } => {
            let status = Command::new("pkexec")
                .args(["apt-get", "remove", "-y", name])
                .status()
                .map_err(|e| RuntimeError::Failed(format!("could not ask for permission: {e}")))?;
            if !status.success() {
                return Err(RuntimeError::Failed(
                    "The package was not removed (permission was refused or the removal failed)."
                        .to_owned(),
                ));
            }
            if delete_data {
                remove_data(data_dir)?;
            }
            Ok(())
        }
        Action::MacTrash { .. } | Action::AppImage { .. } => {
            let trash = home.map(|h| h.join(".Trash")).unwrap_or_default();
            let args = helper_command(action, &trash, 2)
                .ok_or_else(|| RuntimeError::Failed("nothing to run".to_owned()))?;
            if delete_data {
                remove_data(data_dir)?;
            }
            spawn_helper(&args)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(os: &'static str, exe: &str) -> Context {
        Context {
            os,
            exe: PathBuf::from(exe),
            home: Some(PathBuf::from("/Users/me")),
            ..Context::default()
        }
    }

    fn unsupported(a: &Action) -> bool {
        matches!(a, Action::Unsupported { .. })
    }

    #[test]
    fn a_macos_bundle_in_applications_is_trashed_and_nothing_else_is() {
        let ok = choose(&ctx(
            "macos",
            "/Applications/ArchLab.app/Contents/MacOS/archlab",
        ));
        assert_eq!(
            ok,
            Action::MacTrash {
                bundle: PathBuf::from("/Applications/ArchLab.app")
            }
        );
        let user = choose(&ctx(
            "macos",
            "/Users/me/Applications/ArchLab.app/Contents/MacOS/archlab",
        ));
        assert!(matches!(user, Action::MacTrash { .. }));
        for exe in [
            "/Users/me/code/archlab/src-tauri/target/release/bundle/macos/ArchLab.app/Contents/MacOS/archlab",
            "/Users/me/Downloads/ArchLab.app/Contents/MacOS/archlab",
            "/Users/me/code/archlab/src-tauri/target/debug/archlab",
            "/archlab",
        ] {
            assert!(unsupported(&choose(&ctx("macos", exe))), "{exe}");
        }
    }

    #[test]
    fn windows_needs_the_installers_own_uninstaller() {
        let mut c = ctx("windows", "C:/Users/me/AppData/Local/ArchLab/archlab.exe");
        assert!(unsupported(&choose(&c)));
        c.uninstaller = Some(PathBuf::from(
            "C:/Users/me/AppData/Local/ArchLab/uninstall.exe",
        ));
        assert!(matches!(choose(&c), Action::WindowsUninstaller { .. }));
    }

    #[test]
    fn linux_prefers_the_appimage_then_a_known_package_and_refuses_the_rest() {
        let mut c = ctx("linux", "/usr/bin/archlab");
        assert!(unsupported(&choose(&c)));
        c.package = Some("archlab".to_owned());
        assert_eq!(
            choose(&c),
            Action::Package {
                name: "archlab".to_owned()
            }
        );
        c.appimage = Some(PathBuf::from("/home/me/ArchLab.AppImage"));
        assert!(matches!(choose(&c), Action::AppImage { .. }));
        assert!(unsupported(&choose(&ctx(
            "freebsd",
            "/usr/local/bin/archlab"
        ))));
    }

    #[test]
    fn the_confirmation_says_what_will_happen() {
        let data = Path::new("/Users/me/Library/Application Support/dev.archlab.app");
        let mac = describe(
            &Action::MacTrash {
                bundle: PathBuf::from("/Applications/ArchLab.app"),
            },
            data,
        );
        assert!(mac.supported && mac.can_remove_data && !mac.asks_for_password);
        assert!(mac.title.contains("Trash"));
        assert!(mac.data_dir.contains("dev.archlab.app"));

        let win = describe(
            &Action::WindowsUninstaller {
                uninstaller: PathBuf::from("C:/x/uninstall.exe"),
            },
            data,
        );
        assert!(
            win.supported && !win.can_remove_data,
            "the uninstaller asks about data itself"
        );

        let pkg = describe(
            &Action::Package {
                name: "archlab".to_owned(),
            },
            data,
        );
        assert!(pkg.asks_for_password && pkg.title.contains("archlab"));

        let none = describe(
            &Action::Unsupported {
                reason: "Not installed.".to_owned(),
            },
            data,
        );
        assert!(!none.supported && none.title == "Not installed." && none.details.is_empty());
    }

    #[test]
    fn helper_arguments_carry_paths_as_arguments_not_script_text() {
        let evil = "/Applications/Arch Lab\"; rm -rf ~; \".app";
        let action = Action::MacTrash {
            bundle: PathBuf::from(evil),
        };
        let args = helper_command(&action, Path::new("/Users/me/.Trash"), 2).expect("args");
        assert_eq!(&args[..3], ["-c", "sleep 2; mv \"$1\" \"$2\"", "sh"]);
        assert!(!args[1].contains("rm -rf"), "the script has no path in it");
        assert_eq!(args[3], evil);
        assert!(args[4].starts_with("/Users/me/.Trash/"));
        assert!(
            helper_command(&Action::Package { name: "x".into() }, Path::new("/t"), 0).is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_helper_really_moves_a_bundle_and_deletes_an_appimage() {
        let dir = tempfile::tempdir().expect("dir");
        let bundle = dir.path().join("ArchLab.app");
        std::fs::create_dir_all(bundle.join("Contents")).expect("bundle");
        let trash = dir.path().join("Trash");
        std::fs::create_dir(&trash).expect("trash");
        let args = helper_command(
            &Action::MacTrash {
                bundle: bundle.clone(),
            },
            &trash,
            0,
        )
        .expect("args");
        spawn_helper(&args).expect("spawn");
        wait_for(|| trash.join("ArchLab.app").exists());
        assert!(!bundle.exists());

        let image = dir.path().join("ArchLab.AppImage");
        std::fs::write(&image, "x").expect("file");
        let args = helper_command(
            &Action::AppImage {
                file: image.clone(),
            },
            &trash,
            0,
        )
        .expect("args");
        spawn_helper(&args).expect("spawn");
        wait_for(|| !image.exists());
    }

    #[cfg(unix)]
    fn wait_for(done: impl Fn() -> bool) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !done() {
            assert!(std::time::Instant::now() < end, "the helper did not finish");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn deleting_data_removes_the_folder_and_tolerates_a_missing_one() {
        let dir = tempfile::tempdir().expect("dir");
        let data = dir.path().join("data");
        std::fs::create_dir(&data).expect("data");
        std::fs::write(data.join("archlab.db"), "x").expect("db");
        remove_data(&data).expect("removed");
        assert!(!data.exists());
        remove_data(&data).expect("already gone");
        remove_data(Path::new("")).expect("empty path is ignored");
    }

    #[test]
    fn an_unsupported_copy_cannot_be_executed() {
        let action = Action::Unsupported {
            reason: "nope".to_owned(),
        };
        let err = execute(&action, true, Path::new("/nonexistent"), None).expect_err("refused");
        assert!(err.to_string().contains("nope"));
    }
}
