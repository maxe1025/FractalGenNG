//! Project files (`.fgproj`): everything needed to pick a piece of work up
//! again — both the 2D and the 3D fractal with their palettes, the
//! animation timeline, and the overlay and export settings — as
//! human-readable RON.
//!
//! `format_version` guards against files written by a newer FractalGen;
//! fields added in later versions carry `#[serde(default)]` so that older
//! files keep loading.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::export::animation::Keyframe;
use crate::export::camera_path::CameraKeyframe;
use crate::fractal3d::Params3D;
use crate::math_overlay::MathOverlay;
use crate::params::FractalParams;

/// The project-file format this build writes and the newest it can read.
pub const FORMAT_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "fgproj";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Plane,
    Volume,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    /// The FractalGen version that wrote the file (informational).
    #[serde(default)]
    pub written_by: String,
    /// Which fractal was showing.
    pub mode: Mode,
    pub plane: FractalParams,
    pub volume: Params3D,
    pub animation: AnimationSettings,
    pub overlay: MathOverlay,
    pub export: ExportSettings,
    /// 3D fly speed (multiples of the camera's clearance per second).
    #[serde(default = "default_fly_speed")]
    pub fly_speed: f64,
}

fn default_fly_speed() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationSettings {
    pub keys_2d: Vec<Keyframe>,
    pub keys_3d: Vec<CameraKeyframe>,
    pub start: u32,
    pub end: u32,
    pub fps: u32,
    pub looping: bool,
    /// The playhead.
    pub current_frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read or write the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid FractalGen project: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("could not serialize the project: {0}")]
    Serialize(#[from] ron::Error),
    #[error("the project was saved by a newer FractalGen (format {found}, this version reads up to {FORMAT_VERSION})")]
    TooNew { found: u32 },
}

impl Project {
    pub fn to_ron(&self) -> Result<String, ProjectError> {
        Ok(ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?)
    }

    pub fn from_ron(text: &str) -> Result<Self, ProjectError> {
        // Check the version before the full parse, so a newer file reports
        // "too new" rather than whatever field it fails on first.
        #[derive(Deserialize)]
        struct VersionOnly {
            format_version: u32,
        }
        if let Ok(VersionOnly { format_version }) = ron::from_str(text)
            && format_version > FORMAT_VERSION
        {
            return Err(ProjectError::TooNew { found: format_version });
        }
        Ok(ron::from_str(text)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        std::fs::write(path, self.to_ron()?)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::from_ron(&std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::animation::Easing;
    use crate::fractal3d::{Inversion, KIFS_DEFAULT};
    use crate::hp::{required_bits, HpComplex};
    use crate::kinds::{FractalKind, KindParam};

    fn sample() -> Project {
        let mut plane = FractalParams::julia_default();
        plane.zoom = 3.5e40;
        plane.center = HpComplex::from_f64_pair(-0.75, 0.1, required_bits(plane.zoom))
            .add(&HpComplex::from_f64_pair(1.0e-35, -2.0e-36, required_bits(plane.zoom)));
        plane.kind = FractalKind::Julia { seed: [-0.8, 0.156] };
        let mut volume = Params3D::new(KIFS_DEFAULT);
        volume.inversion = Some(Inversion { center: [0.0, 0.1, 0.0], radius: 1.2 });
        volume.camera.roll = 0.3;
        let key_2d = Keyframe { frame: 12, zoom: 50.0, kind_param: KindParam::Seed([0.1, 0.2]), easing: Easing::Linear };
        let key_3d = crate::export::camera_path::CameraKeyframe {
            frame: 30,
            camera: volume.camera,
            easing: Easing::Accelerating { exponent: 0.6 },
        };
        Project {
            format_version: FORMAT_VERSION,
            written_by: "test".to_string(),
            mode: Mode::Volume,
            plane,
            volume,
            animation: AnimationSettings {
                keys_2d: vec![key_2d],
                keys_3d: vec![key_3d],
                start: 5,
                end: 240,
                fps: 24,
                looping: false,
                current_frame: 30,
            },
            overlay: MathOverlay { show_in_viewport: true, ..MathOverlay::default() },
            export: ExportSettings { width: 3840, height: 2160, supersample: 4 },
            fly_speed: 2.5,
        }
    }

    #[test]
    fn round_trips_everything_including_deep_zoom_precision() {
        let project = sample();
        let text = project.to_ron().unwrap();
        let back = Project::from_ron(&text).unwrap();
        // The arbitrary-precision center must survive to its working
        // precision — a deep zoom reopened slightly off-center would show a
        // different image. (Bits an addition left beyond the declared
        // precision aren't kept, so compare digits rather than raw bits:
        // 55 decimals ≈ 183 bits, far finer than this 3.5e40 zoom's pixels.)
        assert_eq!(back.plane.center.to_fixed_decimal_pair(55), project.plane.center.to_fixed_decimal_pair(55));
        assert_eq!(back.plane.center.precision_bits(), project.plane.center.precision_bits());
        assert_eq!(back.plane.kind, project.plane.kind);
        assert_eq!(back.volume.formula, project.volume.formula);
        assert_eq!(back.volume.inversion, project.volume.inversion);
        assert_eq!(back.volume.camera, project.volume.camera);
        assert_eq!(back.animation.keys_2d, project.animation.keys_2d);
        assert_eq!(back.animation.keys_3d, project.animation.keys_3d);
        assert_eq!((back.animation.start, back.animation.end, back.animation.fps), (5, 240, 24));
        assert_eq!(back.overlay, project.overlay);
        assert_eq!(back.export, project.export);
        assert_eq!(back.mode, Mode::Volume);
        assert_eq!(back.fly_speed, 2.5);
        // Once loaded, save → load → save is stable: nothing drifts.
        let again = back.to_ron().unwrap();
        assert_eq!(Project::from_ron(&again).unwrap().to_ron().unwrap(), again);
        assert_eq!(Project::from_ron(&again).unwrap().plane.center, back.plane.center);
    }

    #[test]
    fn newer_format_versions_are_refused_with_a_clear_error() {
        let text = sample().to_ron().unwrap().replacen(
            &format!("format_version: {FORMAT_VERSION}"),
            &format!("format_version: {}", FORMAT_VERSION + 1),
            1,
        );
        assert!(matches!(Project::from_ron(&text), Err(ProjectError::TooNew { found }) if found == FORMAT_VERSION + 1));
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert!(matches!(Project::from_ron("not a project"), Err(ProjectError::Parse(_))));
    }

    #[test]
    fn optional_fields_default_when_missing() {
        let text = sample().to_ron().unwrap();
        let without: String =
            text.lines().filter(|l| !l.contains("fly_speed") && !l.contains("written_by")).collect::<Vec<_>>().join("\n");
        let back = Project::from_ron(&without).unwrap();
        assert_eq!(back.fly_speed, 1.0);
        assert!(back.written_by.is_empty());
    }

    #[test]
    fn saves_and_loads_through_the_file_system() {
        let path = std::env::temp_dir().join(format!("fractalgen-test-{}.{FILE_EXTENSION}", std::process::id()));
        sample().save(&path).unwrap();
        let back = Project::load(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(back.animation.fps, 24);
    }
}
