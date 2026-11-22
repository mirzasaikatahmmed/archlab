//! WebM (VP9) export. Raw RGBA frames are piped to an ffmpeg child process on stdin.
//!
//! ffmpeg is a separate program, looked up in `ARCHLAB_FFMPEG`, then next to the app (the
//! bundled `archlab-ffmpeg` sidecar), then on `PATH`, then in the usual macOS install folders
//! (a Finder-launched app does not get the shell's `PATH`). See ADR-011 and ADR-012.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

use crate::domain::validation::{Issues, ValidationError};
use crate::runtime::RuntimeError;

pub const MAX_FRAMES: u32 = 30 * 600;
const MAX_STDERR_BYTES: u64 = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl VideoSpec {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut issues = Issues::default();
        issues.range_u32("width", self.width, 160, 3840);
        issues.range_u32("height", self.height, 90, 2160);
        issues.check(
            self.width.is_multiple_of(2) && self.height.is_multiple_of(2),
            "width",
            "width and height must be even",
        );
        issues.range_u32("fps", self.fps, 1, 60);
        issues.finish()
    }

    pub fn frame_bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// The bundled program's name. It is not `ffmpeg`, so a Linux package never collides with the
/// distribution's own ffmpeg in `/usr/bin`.
fn sidecar_name() -> &'static str {
    if cfg!(windows) {
        "archlab-ffmpeg.exe"
    } else {
        "archlab-ffmpeg"
    }
}

/// Where package managers put ffmpeg on macOS, which a Finder-launched app cannot find on `PATH`.
fn well_known_dirs() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    } else {
        Vec::new()
    }
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

/// Finds ffmpeg. Pure over its inputs so it can be tested without touching the environment.
pub fn locate_in(
    override_path: Option<OsString>,
    app_dir: Option<&Path>,
    path_var: Option<OsString>,
    extra_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(explicit) = override_path.map(PathBuf::from).filter(|p| is_file(p)) {
        return Some(explicit);
    }
    if let Some(sidecar) = app_dir
        .map(|d| d.join(sidecar_name()))
        .filter(|p| is_file(p))
    {
        return Some(sidecar);
    }
    let on_path = path_var.and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe_name()))
            .find(|p| is_file(p))
    });
    on_path.or_else(|| {
        extra_dirs
            .iter()
            .map(|dir| dir.join(exe_name()))
            .find(|p| is_file(p))
    })
}

pub fn locate() -> Option<PathBuf> {
    let app_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    locate_in(
        std::env::var_os("ARCHLAB_FFMPEG"),
        app_dir.as_deref(),
        std::env::var_os("PATH"),
        &well_known_dirs(),
    )
}

fn args(spec: &VideoSpec, out: &Path) -> Vec<OsString> {
    let mut a: Vec<OsString> = [
        "-y",
        "-loglevel",
        "error",
        "-nostats",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    a.push(format!("{}x{}", spec.width, spec.height).into());
    a.extend(["-r".into(), spec.fps.to_string().into()]);
    a.extend(["-i".into(), "-".into()]);
    a.extend(
        [
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuv420p",
            "-b:v",
            "0",
            "-crf",
            "32",
            "-row-mt",
            "1",
            "-deadline",
            "good",
            "-cpu-used",
            "4",
        ]
        .iter()
        .map(OsString::from),
    );
    a.push(out.as_os_str().to_owned());
    a
}

/// A running ffmpeg process that turns frames into a WebM file.
pub struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
    stderr: Option<JoinHandle<String>>,
    spec: VideoSpec,
    out: PathBuf,
    frames: u32,
}

impl Encoder {
    pub fn start(program: &Path, spec: VideoSpec, out: &Path) -> Result<Self, RuntimeError> {
        spec.validate()?;
        super::check_extension(out, "webm")?;
        let mut child = Command::new(program)
            .args(args(&spec, out))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::Failed(format!("could not start ffmpeg: {e}")))?;
        let stdin = child.stdin.take();
        let stderr = child.stderr.take().map(|pipe| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = pipe.take(MAX_STDERR_BYTES).read_to_string(&mut text);
                text
            })
        });
        Ok(Self {
            child,
            stdin,
            stderr,
            spec,
            out: out.to_path_buf(),
            frames: 0,
        })
    }

    pub fn frames(&self) -> u32 {
        self.frames
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<(), RuntimeError> {
        if rgba.len() != self.spec.frame_bytes() {
            return Err(RuntimeError::Failed(format!(
                "a frame must be {} bytes, got {}",
                self.spec.frame_bytes(),
                rgba.len()
            )));
        }
        if self.frames >= MAX_FRAMES {
            return Err(RuntimeError::Failed(format!(
                "a video can have at most {MAX_FRAMES} frames"
            )));
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| RuntimeError::Failed("the encoder is closed".to_owned()))?;
        stdin
            .write_all(rgba)
            .map_err(|e| RuntimeError::Failed(format!("ffmpeg stopped accepting frames: {e}")))?;
        self.frames += 1;
        Ok(())
    }

    fn stderr_text(&mut self) -> String {
        self.stderr
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
            .trim()
            .to_owned()
    }

    /// Closes the input and waits for ffmpeg. Returns the number of frames written.
    pub fn finish(mut self) -> Result<u32, RuntimeError> {
        drop(self.stdin.take());
        let status = self
            .child
            .wait()
            .map_err(|e| RuntimeError::Failed(format!("ffmpeg did not finish: {e}")))?;
        let message = self.stderr_text();
        if status.success() {
            return Ok(self.frames);
        }
        let _ = std::fs::remove_file(&self.out);
        Err(RuntimeError::Failed(if message.is_empty() {
            format!("ffmpeg failed ({status})")
        } else {
            format!("ffmpeg failed: {message}")
        }))
    }

    /// Stops ffmpeg and removes the partial file.
    pub fn abort(mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.stderr_text();
        let _ = std::fs::remove_file(&self.out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: VideoSpec = VideoSpec {
        width: 160,
        height: 90,
        fps: 30,
    };

    #[test]
    fn checks_the_spec() {
        assert!(SPEC.validate().is_ok());
        assert!(VideoSpec { width: 161, ..SPEC }.validate().is_err());
        assert!(VideoSpec { fps: 0, ..SPEC }.validate().is_err());
        assert!(
            VideoSpec {
                height: 8000,
                ..SPEC
            }
            .validate()
            .is_err()
        );
        assert_eq!(SPEC.frame_bytes(), 160 * 90 * 4);
    }

    #[test]
    fn passes_vp9_webm_arguments_in_order() {
        let a: Vec<String> = args(&SPEC, Path::new("/tmp/o.webm"))
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let at = |flag: &str| a.iter().position(|x| x == flag).expect(flag);
        assert_eq!(a[at("-s") + 1], "160x90");
        assert_eq!(a[at("-r") + 1], "30");
        assert_eq!(a[at("-c:v") + 1], "libvpx-vp9");
        assert!(at("-i") < at("-c:v"), "input options come first");
        assert_eq!(a.last().map(String::as_str), Some("/tmp/o.webm"));
    }

    #[test]
    fn locates_in_override_then_sidecar_then_path_then_known_folders() {
        let dir = tempfile::tempdir().expect("dir");
        let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| dir.path().join(n));
        for (folder, name) in [
            (&a, exe_name()),
            (&b, sidecar_name()),
            (&c, exe_name()),
            (&d, exe_name()),
        ] {
            std::fs::create_dir(folder).expect("mkdir");
            std::fs::write(folder.join(name), "").expect("file");
        }
        let explicit = a.join(exe_name());
        let path_var = std::env::join_paths([&c]).expect("join");
        let extra = [d.clone()];
        assert_eq!(
            locate_in(
                Some(explicit.clone().into_os_string()),
                Some(&b),
                Some(path_var.clone()),
                &extra
            ),
            Some(explicit)
        );
        assert_eq!(
            locate_in(None, Some(&b), Some(path_var.clone()), &extra),
            Some(b.join(sidecar_name()))
        );
        assert_eq!(
            locate_in(None, None, Some(path_var), &extra),
            Some(c.join(exe_name()))
        );
        assert_eq!(
            locate_in(None, None, None, &extra),
            Some(d.join(exe_name()))
        );
        let missing = dir.path().join("nowhere");
        assert_eq!(
            locate_in(Some(missing.into_os_string()), None, None, &[]),
            None
        );
    }

    #[test]
    fn a_plain_ffmpeg_next_to_the_app_is_not_mistaken_for_the_sidecar() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join(exe_name()), "").expect("file");
        assert_eq!(locate_in(None, Some(dir.path()), None, &[]), None);
    }

    #[cfg(unix)]
    mod with_a_fake_ffmpeg {
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        /// A stand-in that writes its stdin to the output path (its last argument).
        fn script(dir: &Path, body: &str) -> PathBuf {
            let path = dir.join("fake-ffmpeg");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            path
        }

        #[test]
        fn pipes_every_frame_to_the_output() {
            let dir = tempfile::tempdir().expect("dir");
            let fake = script(dir.path(), r#"for a; do out="$a"; done; cat > "$out""#);
            let out = dir.path().join("o.webm");
            let mut enc = Encoder::start(&fake, SPEC, &out).expect("start");
            let frame = vec![7_u8; SPEC.frame_bytes()];
            for _ in 0..4 {
                enc.write_frame(&frame).expect("frame");
            }
            assert_eq!(enc.frames(), 4);
            assert_eq!(enc.finish().expect("finish"), 4);
            assert_eq!(
                std::fs::metadata(&out).expect("out").len() as usize,
                4 * SPEC.frame_bytes()
            );
        }

        #[test]
        fn rejects_a_frame_of_the_wrong_size() {
            let dir = tempfile::tempdir().expect("dir");
            let fake = script(dir.path(), r#"for a; do out="$a"; done; cat > "$out""#);
            let out = dir.path().join("o.webm");
            let mut enc = Encoder::start(&fake, SPEC, &out).expect("start");
            assert!(enc.write_frame(&[0; 10]).is_err());
            enc.abort();
            assert!(!out.exists());
        }

        #[test]
        fn reports_ffmpeg_errors_and_removes_the_partial_file() {
            let dir = tempfile::tempdir().expect("dir");
            let fake = script(
                dir.path(),
                r#"for a; do out="$a"; done; echo partial > "$out"; cat > /dev/null; echo "Unknown encoder 'libvpx-vp9'" >&2; exit 1"#,
            );
            let out = dir.path().join("o.webm");
            let enc = Encoder::start(&fake, SPEC, &out).expect("start");
            let err = enc.finish().expect_err("fails");
            assert!(err.to_string().contains("libvpx-vp9"), "{err}");
            assert!(!out.exists());
        }

        #[test]
        fn a_missing_program_is_a_clear_error() {
            let dir = tempfile::tempdir().expect("dir");
            let err = Encoder::start(&dir.path().join("nope"), SPEC, &dir.path().join("o.webm"))
                .err()
                .expect("error");
            assert!(err.to_string().contains("could not start ffmpeg"));
        }

        #[test]
        fn only_webm_files_are_written() {
            let dir = tempfile::tempdir().expect("dir");
            let fake = script(dir.path(), "cat > /dev/null");
            assert!(Encoder::start(&fake, SPEC, &dir.path().join("o.mp4")).is_err());
        }
    }
}
