use std::path::PathBuf;

use fractal_core::Bookmark;

fn bookmarks_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "FractalGen")?;
    let config_dir = dirs.config_dir();
    std::fs::create_dir_all(config_dir).ok()?;
    Some(config_dir.join("bookmarks.ron"))
}

pub fn load_bookmarks() -> Vec<Bookmark> {
    let Some(path) = bookmarks_path() else {
        return Vec::new();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    match ron::from_str(&contents) {
        Ok(bookmarks) => bookmarks,
        Err(err) => {
            log::warn!("failed to parse bookmarks at {}: {err}", path.display());
            Vec::new()
        }
    }
}

pub fn save_bookmarks(bookmarks: &[Bookmark]) {
    let Some(path) = bookmarks_path() else {
        return;
    };
    match ron::ser::to_string_pretty(bookmarks, ron::ser::PrettyConfig::default()) {
        Ok(serialized) => {
            if let Err(err) = std::fs::write(&path, serialized) {
                log::warn!("failed to save bookmarks to {}: {err}", path.display());
            }
        }
        Err(err) => log::warn!("failed to serialize bookmarks: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use fractal_core::hp::HpComplex;
    use fractal_core::{FractalKind, FractalParams};

    use super::*;

    #[test]
    fn bookmark_list_round_trips_through_ron() {
        let bookmarks = vec![
            Bookmark { name: "Mandelbrot home".to_string(), params: FractalParams::default() },
            Bookmark {
                name: "Julia seahorse".to_string(),
                params: FractalParams {
                    kind: FractalKind::Julia { seed: [-0.4, 0.6] },
                    center: HpComplex::from_f64_pair(0.123, -0.456, 128),
                    zoom: 12345.678,
                    ..FractalParams::default()
                },
            },
        ];

        let serialized = ron::ser::to_string_pretty(&bookmarks, ron::ser::PrettyConfig::default())
            .expect("serialize bookmarks");
        let deserialized: Vec<Bookmark> = ron::from_str(&serialized).expect("deserialize bookmarks");

        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].name, "Mandelbrot home");
        assert_eq!(deserialized[1].name, "Julia seahorse");
        assert_eq!(deserialized[1].params.center.to_f64_pair(), (0.123, -0.456));
        assert_eq!(deserialized[1].params.zoom, 12345.678);
        assert_eq!(deserialized[1].params.kind.shader_id(), FractalKind::Julia { seed: [-0.4, 0.6] }.shader_id());
        assert_eq!(deserialized[1].params.kind.julia_seed(), [-0.4, 0.6]);
    }
}
