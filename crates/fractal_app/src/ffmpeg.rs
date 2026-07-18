//! Optional post-export video stitching. Never bundled — this shells out to
//! a system `ffmpeg` if the user has one on `PATH`, the same way `rfd`
//! shells out to native file dialogs, so it carries none of the native-
//! toolchain build risk that ruled out linking against ffmpeg directly (see
//! the project plan's "no ffmpeg bundling" decision). If ffmpeg isn't
//! found, callers fall back to [`command_string`] so the user can run it
//! themselves with zero guesswork.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Whether a `ffmpeg` binary is reachable on `PATH`.
pub fn is_available() -> bool {
    Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// The `frame_%05d.png`-style input pattern ffmpeg's image2 demuxer expects,
/// matching `export_panel.rs`'s `frame_{:05}.png` frame naming exactly.
fn input_pattern(frame_dir: &Path) -> PathBuf {
    frame_dir.join("frame_%05d.png")
}

fn args(frame_dir: &Path, fps: u32, out_path: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-framerate".to_string(),
        fps.to_string(),
        "-i".to_string(),
        input_pattern(frame_dir).display().to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        out_path.display().to_string(),
    ]
}

/// A ready-to-paste shell command, for when ffmpeg isn't on `PATH` here but
/// might be available wherever the user actually runs it.
pub fn command_string(frame_dir: &Path, fps: u32, out_path: &Path) -> String {
    let mut parts = vec!["ffmpeg".to_string()];
    for arg in args(frame_dir, fps, out_path) {
        if arg.chars().any(char::is_whitespace) {
            parts.push(format!("\"{arg}\""));
        } else {
            parts.push(arg);
        }
    }
    parts.join(" ")
}

/// Spawns `ffmpeg` in the background (does not block waiting for it to
/// finish — encoding a long animation can take a while, and the caller is
/// driving an interactive UI). Output is discarded; the caller can only
/// know stitching *started*, not that it succeeded — checking for the
/// output file appearing is the user's signal, same as any other external
/// encode running in a separate window.
pub fn spawn_stitch(frame_dir: &Path, fps: u32, out_path: &Path) -> io::Result<()> {
    Command::new("ffmpeg")
        .args(args(frame_dir, fps, out_path))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_string_uses_zero_padded_frame_pattern_matching_export_naming() {
        let cmd = command_string(Path::new("/out"), 30, Path::new("/out/movie.mp4"));
        assert!(cmd.contains("frame_%05d.png"), "command was: {cmd}");
        assert!(cmd.contains("-framerate 30"));
        assert!(cmd.starts_with("ffmpeg "));
    }

    #[test]
    fn command_string_quotes_paths_with_spaces() {
        let dir = Path::new("/out dir");
        let out = dir.join("movie.mp4");
        let cmd = command_string(dir, 24, &out);
        let expected_input = format!("\"{}\"", input_pattern(dir).display());
        let expected_output = format!("\"{}\"", out.display());
        assert!(cmd.contains(&expected_input), "command was: {cmd}");
        assert!(cmd.contains(&expected_output), "command was: {cmd}");
    }
}
