use device::HDAudio;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Codec {
    device: Arc<Mutex<HDAudio>>,
    id: u8
}

impl Codec {
    pub fn new(device: Arc<Mutex<HDAudio>>, id: u8) -> Arc<Mutex<Self>> {
        let arc = Arc::new(Mutex::new(Codec { device, id }));
        let arc_thread = arc.clone();
        move thread::spawn(|| {
            {
                let device = arc_thread.lock().unwrap();
                device.init();
            }
            loop {}
        })
        
    }

    pub fn init(&self) {
        unsafe { self.device.send_verb((self.id as u32) << 28 | 0xF00 << 8) };
    }
}
