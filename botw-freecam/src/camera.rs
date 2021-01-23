use crate::utils::*;
use nalgebra_glm as glm;
use std::convert::TryInto;
use std::time::Duration;

#[repr(C)]
pub struct GameCamera {
    pub pos: [u32; 3],
    pub focus: [u32; 3],
    // Unknown values (padding)
    pub unk: [u32; 3],
    pub fov: u32,
    pub unk2: [u32; 24],
    pub rot: [u32; 3],
}

impl std::fmt::Debug for GameCamera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ptr = self as *const GameCamera as usize;
        let pos: Vec<f32> = Vec::from(self.pos)
            .into_iter()
            .map(|x| x.to_fbe())
            .collect();

        let focus: Vec<f32> = Vec::from(self.focus)
            .into_iter()
            .map(|x| x.to_fbe())
            .collect();

        f.debug_struct("GameCamera")
            .field("self", &format_args!("{:x}", ptr))
            .field("pos", &pos)
            .field("focus", &focus)
            .field("fov", &(self.fov.to_fbe()))
            .finish()
    }
}

pub trait FromU32BigEndianToFloat {
    fn to_fbe(&self) -> f32;
}
pub trait FromF32ToU32BigEndian {
    fn to_u32(&self) -> u32;
}

impl FromU32BigEndianToFloat for u32 {
    fn to_fbe(&self) -> f32 {
        unsafe { std::mem::transmute(u32::from_be(*self)) }
    }
}

impl FromF32ToU32BigEndian for f32 {
    fn to_u32(&self) -> u32 {
        let val: u32 = unsafe { std::mem::transmute::<f32, u32>(*self) };
        val.to_be()
    }
}

impl GameCamera {
    pub fn consume_input(&mut self, input: &Input) {
        let r_cam_x = self.focus[0].to_fbe() - self.pos[0].to_fbe();
        let r_cam_y = self.focus[1].to_fbe() - self.pos[1].to_fbe();
        let r_cam_z = self.focus[2].to_fbe() - self.pos[2].to_fbe();

        let (r_cam_x, r_cam_z, r_cam_y) = GameCamera::calc_new_focus_point(
            r_cam_x,
            r_cam_z,
            r_cam_y,
            input.delta_focus.0,
            input.delta_focus.1,
        );

        self.pos[0] =
            (self.pos[0].to_fbe() + r_cam_x * input.delta_pos.1 + input.delta_pos.0 * r_cam_z)
                .to_u32();

        self.pos[1] = (self.pos[1].to_fbe() + r_cam_y * input.delta_pos.1 + input.delta_altitude).to_u32();

        self.pos[2] = (self.pos[2].to_fbe() + r_cam_z * input.delta_pos.1
            - input.delta_pos.0 * r_cam_x)
            .to_u32();

        self.focus[0] = (self.pos[0].to_fbe() + r_cam_x).to_u32();
        self.focus[1] = (self.pos[1].to_fbe() + r_cam_y).to_u32();
        self.focus[2] = (self.pos[2].to_fbe() + r_cam_z).to_u32();

        let pos_ = glm::vec3(
            self.pos[0].to_fbe(),
            self.pos[1].to_fbe(),
            self.pos[2].to_fbe(),
        );
        let focus_ = glm::vec3(
            self.focus[0].to_fbe(),
            self.focus[1].to_fbe(),
            self.focus[2].to_fbe(),
        );

        let result = GameCamera::calculate_rotation(focus_, pos_, input.delta_rotation);
        self.rot[0] = result[0].to_u32();
        self.rot[1] = result[1].to_u32();
        self.rot[2] = result[2].to_u32();

        self.fov = input.fov.to_u32();
    }

    pub fn calc_new_focus_point(
        cam_x: f32,
        cam_z: f32,
        cam_y: f32,
        speed_x: f32,
        speed_y: f32,
    ) -> (f32, f32, f32) {
        // use spherical coordinates to add speed
        let theta = cam_z.atan2(cam_x) + speed_x;

        let phi = (cam_x.powi(2) + cam_z.powi(2)).sqrt().atan2(cam_y) + speed_y;

        let r = (cam_x.powi(2) + cam_y.powi(2) + cam_z.powi(2)).sqrt();

        let r_cam_x = r * theta.cos() * phi.sin();
        let r_cam_z = r * theta.sin() * phi.sin();
        let r_cam_y = r * phi.cos();

        (r_cam_x, r_cam_z, r_cam_y)
    }

    pub fn calculate_rotation(focus: glm::Vec3, pos: glm::Vec3, rotation: f32) -> [f32; 3] {
        let up = glm::vec3(0., 1., 0.);

        let m_look_at = glm::look_at(&focus, &pos, &up);
        let direction = {
            let row = m_look_at.row(2);
            glm::vec3(row[0], row[1], row[2])
        };
        // let axis = glm::vec3(0., 0., 1.);
        let m_new = glm::rotate_normalized_axis(&m_look_at, rotation, &direction);

        let result = m_new.row(1);

        [result[0], result[1], result[2]]
    }
}

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

    pub fn calculate_vector(&self, other: &CameraSnapshot) -> ([f32; 3], [f32; 3], [f32; 3]) {
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
        let mut moving_vectors: Vec<CameraSnapshot> = vec![];
        for w in self.windows(2) {
            let (pos, focus, rot) = w[0].calculate_vector(&w[1]);
            moving_vectors.push(
                CameraSnapshot { pos, focus, rot });
        }

        let per_vector_duration = duration.checked_div(moving_vectors.len() as u32).unwrap();
        let sleep_duration = Duration::from_millis(20);
        let fraction = per_vector_duration.as_secs_f32() / sleep_duration.as_secs_f32();

        self[0].set_inplace(gc);

        for vec in moving_vectors {
            let now = std::time::Instant::now();
            let vec = vec.fraction(fraction);
            while now.elapsed() < per_vector_duration {
                vec.move_camera(gc);
                std::thread::sleep(sleep_duration);
            }
        }

    }
}
