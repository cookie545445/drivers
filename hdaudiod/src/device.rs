use std::{thread, ptr, time};
use syscall::Result;
use syscall::io::Dma;

use codec::Codec;

pub const GCTL: u32 = 0x08;
pub const GCTL_RST: u32 = 0x01;

pub const STATESTS: u32 = 0x0E;

pub const CORBLBASE: u32 = 0x40;
pub const CORBUBASE: u32 = 0x44;

pub const CORBWP: u32 = 0x48;
pub const CORBRP: u32 = 0x4A;
pub const CORB_RPRST: u16 = 0x01 << 15;

pub const CORBCTL: u32 = 0x4C;
pub const CORB_RUN: u32 = 0x01 << 1;

pub const CORBSIZE: u32 = 0x4E;

pub const RIRBLBASE: u32 = 0x50;
pub const RIRBUBASE: u32 = 0x54;

pub const RIRBWP: u32 = 0x58;
pub const RIRBWPRST: u16 = 0x01 << 15;

pub const RINTCNT: u32 = 0x5A;

pub const RIRBCTL: u32 = 0x5C;
pub const RIRBDMAEN: u32 = 0x01 << 1;
pub const RINTCTL: u32 = 0x01;

pub const RIRBSTS: u32 = 0x5D;
pub const RIRBSIZE: u32 = 0x5E;

#[repr(packed)]
struct StreamDescriptor {
    stream_ctl: u8,
    _rsvd: u8,
    flags: u8,
    status: u8,
    position: u32,
    buf_length: u32,
    _rsvd2: u8,
    last_valid: u8,
    fifos: u16,
    format: u16,
    bdl_lower: u32,
    bdl_upper: u32
}

pub struct HDAudio {
    base: usize,
    corb: Dma<[u32; 256]>,
    corb_wp: u8,
    rirb: Dma<[u64; 256]>,
    rirb_rp: u8,
pub codecs: Vec<Codec>
}

impl HDAudio {
    pub unsafe fn new(base: usize) -> Result<Self> {
        let mut device = HDAudio {
            base,
            corb: Dma::zeroed()?,
            corb_wp: 0,
            rirb: Dma::zeroed()?,
            rirb_rp: 0,
            codecs: Vec::new()
        };
        device.init();
        Ok(device)
    }

    pub unsafe fn read<T>(&self, register: u32) -> T {
        ptr::read_volatile((self.base + register as usize) as *mut T)
    }

    pub unsafe fn write<T>(&self, register: u32, data: T) -> T {
        ptr::write_volatile((self.base + register as usize) as *mut T, data);
        ptr::read_volatile((self.base + register as usize) as *mut T)
    }

    pub unsafe fn flag(&self, register: u32, flag: u32, value: bool) {
        if value {
            self.write(register, self.read::<u32>(register) | flag);
        } else {
            self.write(register, self.read::<u32>(register) & (0xFFFFFFFF - flag));
        }
    }

    pub unsafe fn init(&mut self) {
        self.flag(GCTL, GCTL_RST, true);
        while self.read::<u32>(GCTL) & GCTL_RST != GCTL_RST {
            thread::yield_now();
        }
        thread::sleep(time::Duration::new(0, 1_000_000));

        self.init_corb();

        self.init_rirb();

    }

    unsafe fn init_corb(&self) {
        self.flag(CORBCTL, CORB_RUN, false);

        if self.read::<u8>(CORBSIZE) & 64 != 64 {
            panic!("CORBSIZE too small")
        }

        self.flag(CORBSIZE, 0x1, false);
        self.flag(CORBSIZE, 0x2, true);

        if self.corb.physical() & 0x7F != 0 {
            panic!("CORB not aligned");
        }

        self.write(CORBLBASE, self.corb.physical() as u32);
        self.write(CORBUBASE, (self.corb.physical() >> 32) as u32);

        self.flag(CORBRP, CORB_RPRST as u32, true);
        while self.read::<u16>(CORBRP) & CORB_RPRST != CORB_RPRST {
            thread::yield_now();
        }

        self.flag(CORBRP, CORB_RPRST as u32, false);
        while self.read::<u16>(CORBRP) & CORB_RPRST == CORB_RPRST {
            thread::yield_now();
        }

        self.write(CORBWP, 0 as u8);
        self.flag(CORBCTL, CORB_RUN, true);
    }

    unsafe fn init_rirb(&self) {
        self.flag(RIRBCTL, RIRBDMAEN, false);

        if self.read::<u8>(RIRBSIZE) & 64 != 64 {
            panic!("RIRBSIZE too small");
        }

        self.flag(RIRBSIZE, 0x1, false);
        self.flag(RIRBSIZE, 0x2, true);

        if self.rirb.physical() & 0x7F != 0 {
            panic!("RIRB not aligned");
        }

        self.write(RIRBLBASE, self.rirb.physical() as u32);
        self.write(RIRBUBASE, (self.rirb.physical() >> 32) as u32);

        self.flag(RIRBWP, RIRBWPRST as u32, true);

        self.write(RINTCNT, 1u8);
        self.flag(RIRBCTL, RINTCTL, true);

        self.flag(RIRBCTL, RIRBDMAEN, true);
    }

    pub unsafe fn send_verb(&mut self, verb: u32) {
        while self.read::<u8>(CORBRP) == self.corb_wp + 1 {
            thread::yield_now();
        }

        self.corb[(self.corb_wp + 1) as usize] = verb;
        self.write(CORBWP, self.corb_wp);
    }
}
