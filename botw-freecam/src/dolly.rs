use crate::utils::*;
use nalgebra_glm as glm;
use std::convert::TryInto;
use std::time::Duration;
use crate::camera::*;
use crate::utils::*;
use winapi::um::winuser;

#[derive(Debug, Clone)]
pub struct CameraSnapshot {
    pos: glm::Vec3,
    focus: glm::Vec3,
    rot: glm::Vec3
}

pub trait Interpolate {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration, loop_it: bool);
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

fn solve_eq(t: f32, p0: glm::Vec3, p1: glm::Vec3, p2: glm::Vec3, p3: glm::Vec3) -> glm::Vec3 {
    let b0 = 0.5 * (-t.powi(3) + 2.*t.powi(2) - t);
    let b1 = 0.5 * (3.*t.powi(3) - 5.*t.powi(2) + 2.);
    let b2 = 0.5 * (-3.*t.powi(3) + 4.*t.powi(2) + t);
    let b3 = 0.5 * (t.powi(3) - t.powi(2));

    p0*b0 + p1*b1 + p2*b2 + p3*b3
}

impl Interpolate for Vec<CameraSnapshot> {
    fn interpolate(&self, gc: &mut GameCamera, duration: Duration, loop_it: bool) {
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

        let sleep_duration = Duration::from_millis(10);

        let fraction = sleep_duration.as_secs_f32() / duration.as_secs_f32();

        self[0].set_inplace(gc);

        macro_rules! bounds {
            ($var:expr) => {
                // TODO: Check if this was the issue with the smooth transition
                if $var < 0 {
                    if loop_it {
                        (self.len() - 1) as i32
                    } else {
                        0
                    }
                } else if $var >= (self.len() - 1) as i32 {
                    if loop_it {
                        $var % (self.len()) as i32
                    } else {
                        (self.len() - 1) as i32
                    }
                } else {
                    $var
                }
            }
        }

        let mut t = 0.;
        let delta_t = if loop_it {
            1. / ((self.len()) as f32)
        } else {
            1. / ((self.len() - 1) as f32)
        };

        'outer: loop {
            let mut t = 0.;
            while t < 1. {
                if check_key_press(winuser::VK_F8) {
                    break 'outer;
                }

                let p: i32 = (t / delta_t) as i32;
                let p0 = bounds!(p - 1);
                let p1 = bounds!(p);
                let p2 = bounds!(p + 1);
                let p3 = bounds!(p + 2);

                let rt = (t - delta_t*(p as f32)) / delta_t;
                let pos = solve_eq(rt, self[p0 as usize].pos, self[p1 as usize].pos, self[p2 as usize].pos, self[p3 as usize].pos);
                let focus = solve_eq(rt, self[p0 as usize].focus, self[p1 as usize].focus, self[p2 as usize].focus, self[p3 as usize].focus);
                let rot = solve_eq(rt, self[p0 as usize].rot, self[p1 as usize].rot, self[p2 as usize].rot, self[p3 as usize].rot);
                let vec = CameraSnapshot { pos, focus, rot };
                vec.set_inplace(gc);
                t += fraction;
                std::thread::sleep(sleep_duration);
            }

            if !loop_it {
                break;
            }
        }

    }
}
