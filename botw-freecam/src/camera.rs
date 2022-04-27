use crate::utils::*;
use nalgebra_glm as glm;

#[derive(Clone, Copy)]
pub struct Vec3BE(pub [FloatBE; 3]);

#[derive(Clone, Copy)]
pub struct FloatBE(u32);

impl From<Vec3BE> for glm::TVec3<f32> {
    fn from(v: Vec3BE) -> Self {
        let v = v.0;
        glm::vec3(v[0].into(), v[1].into(), v[2].into())
    }
}

impl From<glm::TVec3<f32>> for Vec3BE {
    fn from(v: glm::TVec3<f32>) -> Self {
        Vec3BE([v[0].into(), v[1].into(), v[2].into()])
    }
}

#[repr(C)]
pub struct GameCamera {
    pub pos: Vec3BE,
    pub focus: Vec3BE,
    pub rot: Vec3BE,
    pub fov: FloatBE,
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
            .field("fov", &(f32::from(self.fov)))
            .finish()
    }
}

impl From<f32> for FloatBE {
    fn from(val: f32) -> Self {
        FloatBE(val.to_bits().to_be())
    }
}

impl From<FloatBE> for f32 {
    fn from(val: FloatBE) -> f32 {
        f32::from_bits(u32::from_be(val.0))
    }
}

impl GameCamera {
    pub fn consume_input(&mut self, input: &Input) {
        let r_cam_x = f32::from(self.focus.0[0]) - f32::from(self.pos.0[0]);
        let r_cam_y = f32::from(self.focus.0[1]) - f32::from(self.pos.0[1]);
        let r_cam_z = f32::from(self.focus.0[2]) - f32::from(self.pos.0[2]);

        let (r_cam_x, r_cam_z, r_cam_y) = GameCamera::calc_new_focus_point(
            r_cam_x,
            r_cam_z,
            r_cam_y,
            input.delta_focus.0,
            input.delta_focus.1,
        );

        self.pos.0[0] =
            (f32::from(self.pos.0[0]) + r_cam_x * input.delta_pos.1 + input.delta_pos.0 * r_cam_z)
                .into();

        self.pos.0[1] =
            (f32::from(self.pos.0[1]) + r_cam_y * input.delta_pos.1 + input.delta_altitude).into();

        self.pos.0[2] = (f32::from(self.pos.0[2]) + r_cam_z * input.delta_pos.1
            - input.delta_pos.0 * r_cam_x)
            .into();

        self.focus.0[0] = (f32::from(self.pos.0[0]) + r_cam_x).into();
        self.focus.0[1] = (f32::from(self.pos.0[1]) + r_cam_y).into();
        self.focus.0[2] = (f32::from(self.pos.0[2]) + r_cam_z).into();

        let pos_ = glm::Vec3::from(self.pos);
        let focus_ = glm::Vec3::from(self.focus);
        let result = GameCamera::calculate_rotation(focus_, pos_, input.delta_rotation);
        self.rot = result.into();

        self.fov = input.fov.into();
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

        let r = 5.;

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

    // Just kept around for legacy purposes.
    #[allow(dead_code)]
    pub fn clamp_distance(&mut self, point: &glm::Vec3) {
        let cp = glm::Vec3::from(self.pos);
        let cf = glm::Vec3::from(self.focus);
        let delta_view = cf - cp;
        let distance = glm::l2_norm(&(point - cp));
        if distance > 400. {
            let norm = glm::normalize(&(cp - point));
            let new_point: glm::Vec3 = *point + norm * 380.;

            self.pos = new_point.into();
            self.focus = Vec3BE::from(new_point + delta_view);
        }
    }
}
