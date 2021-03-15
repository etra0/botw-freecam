use crate::utils::*;
use nalgebra_glm as glm;

#[derive(Clone, Copy)]
pub struct Vec3BE(pub [u32; 3]);

impl From<Vec3BE> for glm::TVec3<f32> {
    fn from(v: Vec3BE) -> Self {
        let v = v.0;
        glm::vec3(v[0].to_fbe(), v[1].to_fbe(), v[2].to_fbe())
    }
}

impl From<glm::TVec3<f32>> for Vec3BE {
    fn from(v: glm::TVec3<f32>) -> Self {
        Vec3BE([v[0].to_u32(), v[1].to_u32(), v[2].to_u32()])
    }
}

#[repr(C)]
pub struct GameCamera {
    pub pos: Vec3BE,
    pub focus: Vec3BE,
    // Unknown values (padding)
    pub _unk: Vec3BE,
    pub fov: u32,
    pub _unk2: [u32; 24],
    pub rot: Vec3BE,
}

impl std::fmt::Debug for GameCamera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ptr = self as *const GameCamera as usize;
        let pos: glm::Vec3 = self.pos.into();
        let focus: glm::Vec3 = self.focus.into();

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
        f32::from_bits(u32::from_be(*self))
    }
}

impl FromF32ToU32BigEndian for f32 {
    fn to_u32(&self) -> u32 {
        let val: u32 = (*self).to_bits();
        val.to_be()
    }
}

impl GameCamera {
    pub fn consume_input(&mut self, input: &Input) {
        let r_cam_x = self.focus.0[0].to_fbe() - self.pos.0[0].to_fbe();
        let r_cam_y = self.focus.0[1].to_fbe() - self.pos.0[1].to_fbe();
        let r_cam_z = self.focus.0[2].to_fbe() - self.pos.0[2].to_fbe();

        let (r_cam_x, r_cam_z, r_cam_y) = GameCamera::calc_new_focus_point(
            r_cam_x,
            r_cam_z,
            r_cam_y,
            input.delta_focus.0,
            input.delta_focus.1,
        );

        self.pos.0[0] =
            (self.pos.0[0].to_fbe() + r_cam_x * input.delta_pos.1 + input.delta_pos.0 * r_cam_z)
                .to_u32();

        self.pos.0[1] =
            (self.pos.0[1].to_fbe() + r_cam_y * input.delta_pos.1 + input.delta_altitude).to_u32();

        self.pos.0[2] = (self.pos.0[2].to_fbe() + r_cam_z * input.delta_pos.1
            - input.delta_pos.0 * r_cam_x)
            .to_u32();

        self.focus.0[0] = (self.pos.0[0].to_fbe() + r_cam_x).to_u32();
        self.focus.0[1] = (self.pos.0[1].to_fbe() + r_cam_y).to_u32();
        self.focus.0[2] = (self.pos.0[2].to_fbe() + r_cam_z).to_u32();

        let pos_ = glm::Vec3::from(self.pos);
        let focus_ = glm::Vec3::from(self.focus);
        let result = GameCamera::calculate_rotation(focus_, pos_, input.delta_rotation);
        self.rot = result.into();

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

    pub fn calculate_rotation(focus: glm::Vec3, pos: glm::Vec3, rotation: f32) -> glm::TVec3<f32> {
        let up = glm::vec3(0., 1., 0.);

        // Calculate the matrix from the look_at
        let m_look_at = glm::look_at(&focus, &pos, &up);

        // Get the focus-pos axis
        let direction = m_look_at.fixed_rows::<glm::U1>(2).transpose().xyz();

        // Calculate the rotation from the focus-pos axis
        let m_new = glm::rotate_normalized_axis(&m_look_at, -rotation, &direction);

        // Get the new up-vector
        m_new.fixed_rows::<glm::U1>(1).transpose().xyz()
    }
}
