//! Editing operations on a keyframe track — a `Vec` of keyframes kept
//! sorted by frame with at most one keyframe per frame — shared by the 2D
//! zoom/parameter keyframes and the 3D camera keyframes.

use super::animation::{Easing, Keyframe};
use super::camera_path::CameraKeyframe;

/// What a timeline needs from a keyframe: its frame, and the easing of the
/// segment leading into it.
pub trait Keyed: Copy {
    fn frame(&self) -> u32;
    fn set_frame(&mut self, frame: u32);
    fn easing_mut(&mut self) -> &mut Easing;
}

impl Keyed for Keyframe {
    fn frame(&self) -> u32 {
        self.frame
    }
    fn set_frame(&mut self, frame: u32) {
        self.frame = frame;
    }
    fn easing_mut(&mut self) -> &mut Easing {
        &mut self.easing
    }
}

impl Keyed for CameraKeyframe {
    fn frame(&self) -> u32 {
        self.frame
    }
    fn set_frame(&mut self, frame: u32) {
        self.frame = frame;
    }
    fn easing_mut(&mut self) -> &mut Easing {
        &mut self.easing
    }
}

/// Index of the keyframe sitting exactly on `frame`.
pub fn index_at_frame<K: Keyed>(keys: &[K], frame: u32) -> Option<usize> {
    keys.binary_search_by_key(&frame, |k| k.frame()).ok()
}

/// Inserts `key`, replacing a keyframe already on the same frame. Returns
/// its index.
pub fn insert_or_replace<K: Keyed>(keys: &mut Vec<K>, key: K) -> usize {
    match keys.binary_search_by_key(&key.frame(), |k| k.frame()) {
        Ok(i) => {
            keys[i] = key;
            i
        }
        Err(i) => {
            keys.insert(i, key);
            i
        }
    }
}

/// Moves keyframe `index` to `to_frame`, keeping the track sorted. A move
/// onto a frame another keyframe already occupies is refused. Returns the
/// keyframe's (possibly new) index.
pub fn move_key<K: Keyed>(keys: &mut Vec<K>, index: usize, to_frame: u32) -> usize {
    if keys[index].frame() == to_frame || index_at_frame(keys, to_frame).is_some() {
        return index;
    }
    let mut key = keys.remove(index);
    key.set_frame(to_frame);
    insert_or_replace(keys, key)
}

/// The first keyframe frame after `frame`.
pub fn next_key_frame<K: Keyed>(keys: &[K], frame: u32) -> Option<u32> {
    keys.iter().map(|k| k.frame()).find(|&f| f > frame)
}

/// The last keyframe frame before `frame`.
pub fn previous_key_frame<K: Keyed>(keys: &[K], frame: u32) -> Option<u32> {
    keys.iter().rev().map(|k| k.frame()).find(|&f| f < frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::animation::Easing;
    use crate::kinds::KindParam;

    fn key(frame: u32, zoom: f64) -> Keyframe {
        Keyframe { frame, zoom, kind_param: KindParam::None, easing: Easing::Linear }
    }

    fn frames(keys: &[Keyframe]) -> Vec<u32> {
        keys.iter().map(|k| k.frame).collect()
    }

    #[test]
    fn insert_keeps_order_and_replaces_same_frame() {
        let mut keys = Vec::new();
        assert_eq!(insert_or_replace(&mut keys, key(20, 1.0)), 0);
        assert_eq!(insert_or_replace(&mut keys, key(5, 2.0)), 0);
        assert_eq!(insert_or_replace(&mut keys, key(40, 3.0)), 2);
        assert_eq!(frames(&keys), vec![5, 20, 40]);
        assert_eq!(insert_or_replace(&mut keys, key(20, 9.0)), 1);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[1].zoom, 9.0);
    }

    #[test]
    fn move_resorts_and_refuses_occupied_frames() {
        let mut keys = vec![key(0, 1.0), key(10, 2.0), key(20, 3.0)];
        // Past a neighbour: re-sorted, index follows the key.
        let i = move_key(&mut keys, 0, 15);
        assert_eq!(frames(&keys), vec![10, 15, 20]);
        assert_eq!((i, keys[i].zoom), (1, 1.0));
        // Onto an occupied frame: nothing happens.
        assert_eq!(move_key(&mut keys, i, 20), 1);
        assert_eq!(frames(&keys), vec![10, 15, 20]);
    }

    #[test]
    fn jumps_find_neighbouring_keys() {
        let keys = vec![key(3, 1.0), key(10, 1.0), key(25, 1.0)];
        assert_eq!(next_key_frame(&keys, 10), Some(25));
        assert_eq!(next_key_frame(&keys, 25), None);
        assert_eq!(previous_key_frame(&keys, 10), Some(3));
        assert_eq!(previous_key_frame(&keys, 3), None);
        assert_eq!(index_at_frame(&keys, 25), Some(2));
        assert_eq!(index_at_frame(&keys, 24), None);
    }
}
