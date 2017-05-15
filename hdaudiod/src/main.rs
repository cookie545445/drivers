extern crate event;
extern crate syscall;

use std::env;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::io::{Read, Write, Result};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use event::EventQueue;
use syscall::flag::MAP_WRITE;

mod device;
mod codec;

use codec::Codec;

fn main() {
    let mut args = env::args().skip(1);

    let mut name = args.next().expect("no name provided");
    name.push_str("_hdaudio");

    let lower_str = args.next().expect("no BAR0 provided");
    let upper_str = args.next().expect("no BAR1 provided");
    let bar_str = format!("{}{}", upper_str, lower_str);
    let bar = usize::from_str_radix(&bar_str, 16).expect("failed to parse address");

    let irq_str = args.next().expect("no IRQ provided");
    let irq = irq_str.parse::<u8>().expect("failed to parse IRQ");

    println!(" + HD Audio {} on: {:X}, IRQ: {}", name, bar, irq);

    // Daemonize
    if unsafe { syscall::clone(0).unwrap() } == 0 {
        let address = unsafe { syscall::physmap(bar, 0x180, MAP_WRITE).expect("failed to map address") };        
        let dev_lock = unsafe { Arc::new(Mutex::new(device::HDAudio::new(address).expect("failed to allocate device"))) };

        let mut event_queue = EventQueue::<usize>::new().expect("failed to create event queue");
        let mut irq_file = File::open(format!("irq:{}", irq)).expect("failed to open irq file");

        event_queue.add(irq_file.as_raw_fd(), move |_count: usize| -> Result<Option<usize>> {
            let mut irq = [0u8; 8];
            irq_file.read(&mut irq)?;
            println!("Received interrupt: {:?}", irq);
            Ok(None)
        }).expect("failed to catch events on irq file");

        let statests = unsafe {
            let dev = dev_lock.lock().unwrap();
            dev.read::<u16>(device::STATESTS)
        };
        for i in 0u8..15 {
            let locked = device.lock().unwrap();
            if statests & (1 << i) != 0 {
                println!("   - SDI {} present", i);
                let arc_new = dev_lock.clone();
                locked.codecs.push(Codec::new(arc_new, i).init());
            }
        }
    }
}
