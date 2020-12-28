use crate::utils::*;

#[repr(C)]
pub struct GameCamera {
    pos: [u32; 3],
    focus: [u32; 3],
}

impl std::fmt::Debug for GameCamera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ptr = self as *const GameCamera as usize;
        let pos: Vec<f32> = Vec::from(self.pos)
            .into_iter()
            .map(|x| x.to_fbe())
            .collect();

        let focus: Vec<f32>= Vec::from(self.focus)
            .into_iter()
            .map(|x| x.to_fbe())
            .collect();

        f.debug_struct("GameCamera")
            .field("self", &format_args!("{:x}", ptr))
            .field("pos", &pos)
            .field("focus", &focus)
            .finish()
    }
}

trait FromU32BigEndianToFloat {
    fn to_fbe(&self) -> f32;
}
trait FromF32ToU32BigEndian {
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

        self.pos[0] = (self.pos[0].to_fbe() + r_cam_x * input.delta_pos.1 + input.delta_pos.0 * r_cam_z).to_u32();
        self.pos[1] = (self.pos[1].to_fbe() + r_cam_y * input.delta_pos.1).to_u32();

        self.pos[2] = (self.pos[2].to_fbe() + r_cam_z * input.delta_pos.1 - input.delta_pos.0 * r_cam_x).to_u32();

        self.focus[0] = (self.pos[0].to_fbe() + r_cam_x).to_u32();
        self.focus[1] = (self.pos[1].to_fbe() + r_cam_y).to_u32();
        self.focus[2] = (self.pos[2].to_fbe() + r_cam_z).to_u32();

        // let focus_ = glm::vec3(self.focus[0], self.focus[1], self.focus[2]);
        // let pos_ = glm::vec3(self.pos[0], self.pos[1], self.pos[2]);

        // let result = Camera::calculate_rotation(focus_, pos_, input.delta_rotation);
        // self.rot[0] = result[0];
        // self.rot[1] = result[1];
        // self.rot[2] = result[2];

        // self.fov = input.fov;
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
}