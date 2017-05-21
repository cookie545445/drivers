use std::{thread, ptr, time};
use std::rc::Rc;
use std::cell::RefCell;
use syscall::Result;
use syscall::io::Dma;

use codec::Codec;

pub const GCAP: u32 = 0x00;
pub const _64OK: u32 = 0x01;

pub const VMIN: u32 = 0x02;
pub const VMAJ: u32 = 0x03;

pub const GCTL: u32 = 0x08;
pub const CRST: u32 = 0x01;
pub const GCTL_UNSOL: u32 = 0x01 << 8;

pub const WAKEEN: u32 = 0x0C;

pub const STATESTS: u32 = 0x0E;

pub const INTCTL: u32 = 0x20;
pub const GIE: u32 = 0x01 << 31;
pub const CIE: u32 = 0x01 << 30;

pub const INTSTS: u32 = 0x24;

pub const CORBLBASE: u32 = 0x40;
pub const CORBUBASE: u32 = 0x44;

pub const CORBWP: u32 = 0x48;
pub const CORBRP: u32 = 0x4A;
pub const CORB_RPRST: u16 = 0x01 << 15;

pub const CORBCTL: u32 = 0x4C;
pub const CORB_RUN: u8 = 0x01 << 1;

pub const CORBSTS: u32 = 0x4D;

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
pub const RINTFL: u32 = 0x01;
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
    long_address: bool,
    base: usize,
    corb: Dma<[u32; 256]>,
    corb_wp: u8,
    rirb: Dma<[u64; 256]>,
    rirb_rp: u8
}

impl HDAudio {
    pub unsafe fn new(base: usize) -> Result<Self> {
        let mut device = HDAudio {
            long_address: false,
            base,
            corb: Dma::zeroed()?,
            corb_wp: 0,
            rirb: Dma::zeroed()?,
            rirb_rp: 0
        };
        device.init();
        Ok(device)
    }

    pub fn irq(&self) -> bool {
        let intsts = self.read::<u32>(INTSTS);
        println!("intsts is {:032b}", intsts);
        intsts != 0
    }

    pub fn read<T>(&self, register: u32) -> T {
        if register > 0x180 {
            panic!("out of bounds register access");
        }
        unsafe {
            ptr::read_volatile((self.base + register as usize) as *mut T)
        }
    }

    pub fn write<T>(&self, register: u32, data: T) -> T {
        if register > 0x180 {
            panic!("out of bounds register access");
        }
        unsafe {
            ptr::write_volatile((self.base + register as usize) as *mut T, data);
            ptr::read_volatile((self.base + register as usize) as *mut T)
        }
    }

    pub fn flag(&self, register: u32, flag: u32, value: bool) {
        if value {
            self.write(register, self.read::<u32>(register) | flag);
        } else {
            self.write(register, self.read::<u32>(register) & (0xFFFFFFFF - flag));
        }
    }

    pub fn init(&mut self) {
        self.flag(GCTL, CRST, false);
        while self.read::<u32>(GCTL) & CRST != 0 {
            thread::yield_now();
        }
        thread::sleep(time::Duration::new(0, 1_000_000));

        self.flag(GCTL, CRST, true);
        while self.read::<u32>(GCTL) & CRST != CRST {
            thread::yield_now();
        }

        println!("Major Version: {}, Minor Version: {}", self.read::<u8>(VMAJ), self.read::<u8>(VMIN));

        self.long_address = self.read::<u32>(GCAP) & _64OK == _64OK;

        thread::sleep(time::Duration::new(1, 000_000));

        self.flag(GCTL, GCTL_UNSOL, true);
        self.flag(INTCTL, GIE, true);
        self.flag(INTCTL, CIE, true);

        self.init_corb();
        self.init_rirb();
    }

    fn init_corb(&self) {
        self.write::<u8>(CORBCTL, 0);

        if self.read::<u8>(CORBSIZE) & 64 != 64 {
            panic!("CORBSIZE too small")
        }

        self.write(CORBSIZE, 2u8);

        if self.corb.physical() & 0x7F != 0 {
            panic!("CORB not aligned");
        }

        if self.long_address == true {
            self.write(CORBLBASE, self.corb.physical() as u32);
            self.write(CORBUBASE, (self.corb.physical() >> 32) as u32);
        } else {
            if self.corb.physical() > u32::max_value() as usize {
                panic!("CORB allocated above 32 bit address space");
            } else {
                self.write(CORBLBASE, self.corb.physical() as u32);
            }
        }

        self.flag(CORBRP, CORB_RPRST as u32, true);
        while self.read::<u16>(CORBRP) & CORB_RPRST != CORB_RPRST {
            thread::yield_now();
        }

        self.flag(CORBRP, CORB_RPRST as u32, false);
        while self.read::<u16>(CORBRP) & CORB_RPRST == CORB_RPRST {
            thread::yield_now();
        }

        self.write(CORBWP, 0 as u16);
        self.write(CORBCTL, CORB_RUN);
        while self.read::<u8>(CORBCTL) & CORB_RUN == 0 {
            thread::yield_now();
        }
        println!("CORBCTL: {:08b}\nCORBSTS: {:08b}", self.read::<u8>(CORBCTL), self.read::<u8>(CORBSTS));
    }

    fn init_rirb(&self) {
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
        self.flag(RIRBCTL, RIRBDMAEN, true);
    }

    pub fn send_verb(&mut self, verb: u32) {
        self.corb_wp += 1;
        while self.read::<u8>(CORBRP) == self.corb_wp {
            thread::yield_now();
        }

        self.corb[(self.corb_wp) as usize] = verb;
        self.write(CORBWP, self.corb_wp);
        println!("Written new CORB pointer: {}, read pointer is {}", self.read::<u8>(CORBWP), self.read::<u8>(CORBRP));
    }

    pub fn read_response(&mut self) -> Option<u64> {
        let mut ret = None;
        if self.read::<u8>(RIRBWP) > self.rirb_rp {
            self.rirb_rp += 1;
            ret = Some(self.rirb[self.rirb_rp as usize]);
        }
        ret
    }
}
