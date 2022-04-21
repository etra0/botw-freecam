use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, convert::TryInto, net::UdpSocket};
use flume::{Sender, Receiver};

#[derive(Debug)]
pub struct PackedCameraData {
    pub angle_x: f32,
    pub angle_y: f32,
    pub angle_z: f32,

    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,

    pub up_x: f32,
    pub up_y: f32,
    pub up_z: f32,
}

pub struct BlenderServer {
    socket: UdpSocket,
    sender: Sender<PackedCameraData>,
    should_run: Arc<AtomicBool>,
}

pub struct BlenderReceiver {
    pub receiver: Receiver<PackedCameraData>,
    pub should_run: Arc<AtomicBool>,
}

impl BlenderServer {
    pub fn new(sender: Sender<PackedCameraData>, should_run: Arc<AtomicBool>) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:54321").unwrap();
        socket.set_nonblocking(true).unwrap();
        Self { socket, sender, should_run }
    }

    pub fn start_listening(&mut self) -> Result<(), flume::TrySendError<PackedCameraData>> {
        // TODO: Make the buffer smaller.
        let mut buf = [0_u8; 128];
        self.should_run.store(true, Ordering::Relaxed);

        while self.should_run.load(Ordering::Relaxed) {
            match self.socket.recv(&mut buf) {
                Ok(_) => {
                    let pcd = PackedCameraData::new(&buf);
                    match self.sender.try_send(pcd) {
                        Ok(_) | Err(flume::TrySendError::Full(_)) => (),
                        e => return e,
                    }
                },
                Err(ref e) if e.kind() != std::io::ErrorKind::WouldBlock => {
                    log::error!("Something went wrong: {}", e);
                },
                _ => {},
            }
        }

        Ok(())
    }
}

impl PackedCameraData {
    // TODO: Maybe we could optimize by doing transmute?
    fn new(array: &[u8]) -> Self {
        let angle_x = f32::from_le_bytes(array[0..4].try_into().unwrap());
        let angle_y = f32::from_le_bytes(array[4..8].try_into().unwrap());
        let angle_z = f32::from_le_bytes(array[8..12].try_into().unwrap());
        let pos_x = f32::from_le_bytes(array[12..16].try_into().unwrap());
        let pos_y = f32::from_le_bytes(array[16..20].try_into().unwrap());
        let pos_z = f32::from_le_bytes(array[20..24].try_into().unwrap());

        let up_x = f32::from_le_bytes(array[24..28].try_into().unwrap());
        let up_y = f32::from_le_bytes(array[28..32].try_into().unwrap());
        let up_z = f32::from_le_bytes(array[32..36].try_into().unwrap());

        return Self { angle_x, angle_y, angle_z, pos_x, pos_y, pos_z, up_x, up_y, up_z };
    }
}
