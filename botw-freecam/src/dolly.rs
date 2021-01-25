use crate::utils::*;
use nalgebra_glm as glm;
use std::convert::TryInto;
use std::time::Duration;
use crate::camera::*;

#[derive(Debug, Clone)]
pub struct CameraSnapshot {
    pos: glm::Vec3,
    focus: glm::Vec3,
    rot: glm::Vec3
}

pub trait Interpolate {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration);
}

impl CameraSnapshot {
    pub fn new(gc: &GameCamera) -> Self {
        let pos: glm::Vec3 = [gc.pos[0].to_fbe(), gc.pos[1].to_fbe(), gc.pos[2].to_fbe()].into();
        let focus: glm::Vec3 = [gc.focus[0].to_fbe(), gc.focus[1].to_fbe(), gc.focus[2].to_fbe()].into();
        let rot: glm::Vec3 = [gc.rot[0].to_fbe(), gc.rot[1].to_fbe(), gc.rot[2].to_fbe()].into();

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

    pub fn fraction(&self, fraction: f32) -> Self {
        Self {
            pos: self.pos / fraction,
            focus: self.focus / fraction,
            rot: self.rot / fraction,
        }
    }

}

impl Interpolate for Vec<CameraSnapshot> {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration) {
        // Distance vectors (relative vectors that will be added to the initial
        // camera position
        let mut moving_vectors: Vec<CameraSnapshot> = vec![];

        // Calculate every distance vector between the `CameraSnapshot`s
        for w in self.windows(2) {
            let pos = w[1].pos - w[0].pos;
            let focus = w[1].focus - w[0].focus;
            let rot = w[1].rot - w[0].rot;
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
