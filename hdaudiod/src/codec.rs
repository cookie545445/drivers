use device::HDAudio;
use std::{thread, time};
use std::sync::mpsc::Sender;
use spmc::Receiver;

const SOL: u64 = 0x01 << 36;

const GET_PARAM: u32 = 0xF00_00;

const SUB_NODE_COUNT: u32 = 0x04;

pub struct Codec {
    verb_sender: Sender<u32>,
    resp_recv: Receiver<u64>,
    id: u8,
    num_subs: u8,
    sub_start: u8
}

impl Codec {
    pub fn new(verb_sender: Sender<u32>, resp_recv: Receiver<u64>, id: u8) {
        Codec { verb_sender,
                resp_recv,
                id,
                num_subs: 0,
                sub_start: 0
            }.start();
    }

    fn start(mut self) {
        thread::spawn(move || {
            self.init();
            loop { self.run(); }
        });
    }

    fn init(&mut self) {
        self.verb_sender.send((self.id as u32) << 28 | GET_PARAM | SUB_NODE_COUNT);
        if let Ok(resp) = self.resp_recv.recv() {
            if self.is_mine(resp) && resp & SOL == SOL {
                self.num_subs = resp as u8;
                self.sub_start = (resp >> 16) as u8;
            }
        }
        println!("num_subs: {}, sub_start: {}", self.num_subs, self.sub_start);
    }

    fn is_mine(&self, resp: u64) -> bool {
        resp & (self.id as u64) << 32 == (self.id as u64) << 32
    }

    fn run(&self) {}
}
