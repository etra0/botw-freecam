use crate::utils::*;
use nalgebra_glm as glm;
use std::convert::TryInto;
use std::time::Duration;
use crate::camera::*;

#[derive(Debug, Clone)]
pub struct CameraSnapshot {
    pos: [f32; 3],
    focus: [f32; 3],
    rot: [f32; 3]
}

pub trait Interpolate {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration);
}

impl CameraSnapshot {
    pub fn new(gc: &GameCamera) -> Self {
        let mut pos = [0f32; 3];
        let mut focus = [0f32; 3];
        let mut rot = [0f32; 3];

        // We zip both pos and focus to just do one iteration when copying.
        let iterable = pos
            .iter_mut()
            .zip(focus.iter_mut())
            .zip(rot.iter_mut())
            .enumerate();

        for (i, ((_pos, _focus), _rot)) in iterable {
            *_pos = gc.pos[i].to_fbe();
            *_focus = gc.focus[i].to_fbe();
            *_rot = gc.rot[i].to_fbe();
        }

        Self {
            pos, focus, rot
        }
    }

    pub fn set_inplace(&self, gc: &mut GameCamera) {
        let iterable = gc.pos
            .iter_mut()
            .zip(gc.focus.iter_mut())
            .zip(gc.rot.iter_mut())
            .enumerate();

        for (i, ((_pos, _focus), _rot)) in iterable {
            *_pos = self.pos[i].to_u32();
            *_focus = self.focus[i].to_u32();
            *_rot = self.rot[i].to_u32();
        }
    }

    pub fn move_camera(&self, gc: &mut GameCamera) {
        let iterable = gc.pos
            .iter_mut()
            .zip(gc.focus.iter_mut())
            .zip(gc.rot.iter_mut())
            .enumerate();

        for (i, ((_pos, _focus), _rot)) in iterable {
            *_pos = ((*_pos).to_fbe() + self.pos[i]).to_u32();
            *_focus = ((*_focus).to_fbe() + self.focus[i]).to_u32();
            *_rot = ((*_rot).to_fbe() + self.rot[i]).to_u32();
        }
    }

    pub fn distance_vector(&self, other: &CameraSnapshot) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let mut pos = [0f32; 3];
        let mut focus = [0f32; 3];
        let mut rot = [0f32; 3];

        for i in 0..3 {
            pos[i] = -(self.pos[i] - other.pos[i]);
            focus[i] = -(self.focus[i] - other.focus[i]);
            rot[i] = -(self.rot[i] - other.rot[i]);
        }

        return (pos, focus, rot)
    }

    pub fn fraction(&self, fraction: f32) -> Self {
        let mut vec = self.clone();

        let iterable = vec.pos
            .iter_mut()
            .zip(vec.focus.iter_mut())
            .zip(vec.rot.iter_mut());
        for ((pos, focus), rot) in iterable {
            *pos = *pos / fraction;
            *focus = *focus / fraction;
            *rot = *rot / fraction;
        }
        return vec;
    }
}

impl Interpolate for Vec<CameraSnapshot> {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration) {
        // Distance vectors (relative vectors that will be added to the initial
        // camera position
        let mut moving_vectors: Vec<CameraSnapshot> = vec![];

        // Calculate every distance vector between the `CameraSnapshot`s
        for w in self.windows(2) {
            let (pos, focus, rot) = w[0].distance_vector(&w[1]);
            moving_vectors.push(
                CameraSnapshot { pos, focus, rot });
        }

        // Currently split the duration of every vector evenly, which causes
        // that longer distances will be achieved more "fast"
        // TODO: maybe make the duration relative to its distance?
        let per_vector_duration = duration.checked_div(moving_vectors.len() as u32).unwrap();

        let sleep_duration = Duration::from_millis(20);

        // Fraction will be the number of times we need to divide
        // our relative vector to add it relatively to the camera position
        let fraction = per_vector_duration.as_secs_f32() / sleep_duration.as_secs_f32();

        self[0].set_inplace(gc);

        for vec in moving_vectors {
            let now = std::time::Instant::now();
            let vec = vec.fraction(fraction);
            // Add the relative vector until the time quota is met.
            while now.elapsed() < per_vector_duration {
                vec.move_camera(gc);
                std::thread::sleep(sleep_duration);
            }
        }

    }
}
