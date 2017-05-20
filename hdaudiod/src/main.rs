extern crate event;
extern crate syscall;
extern crate spmc;

use std::{env, thread};
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::io::{Read, Write, Result};
use std::sync::mpsc;
use std::rc::Rc;
use std::cell::RefCell;

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
        let mut device = Rc::new(RefCell::new(unsafe { device::HDAudio::new(address).expect("failed to allocate device") }));
        let (verb_tx, verb_rx) = mpsc::channel::<u32>();
        let (resp_tx, resp_rx) = spmc::channel::<u64>();

//        let mut event_queue = EventQueue::<usize>::new().expect("failed to create event queue");
        let mut irq_file = File::open(format!("irq:{}", irq)).expect("failed to open irq file");

        let mut device_irq = device.clone();
/*        event_queue.add(irq_file.as_raw_fd(), move |_count: usize| -> Result<Option<usize>> {
            let mut irq = [0u8; 8];
            irq_file.read(&mut irq)?;
            let mut dev = device_irq.borrow_mut();
            if dev.irq() {
                println!("Interrupt {:?} is from HD Audio controller", irq);
                if dev.read::<u8>(device::RIRBSTS) | 1 == 1 {
                    println!("Interrupt is from RIRB");
                    let mut response: u64 = 0;
                    if let Some(v) = dev.read_response() {
                        response = v;
                    } else {
                        println!("what the hell");
                    }
                    println!("Received response: {:X}", response);
                    resp_tx.send(response);
                    dev.flag(device::RIRBSTS, 1, true);
                }
            }
            Ok(None)
        }).expect("failed to catch events on irq file");
*/
        let mut statests: u16 = 0;
        {
            let dev = device.borrow();
            let _statests = {
                dev.read::<u16>(device::STATESTS)
            };
            dev.flag(device::STATESTS, statests as u32, true);
            dev.flag(device::WAKEEN, statests as u32, true);
            statests = _statests;
        }

        for i in 0u8..15 {
            if statests & (1 << i) != 0 {
                println!("   - SDI {} present", i);
                Codec::new(verb_tx.clone(), resp_rx.clone(), i);
            }
        }
        

        loop {
            let mut dev = device.borrow_mut();
            if let Ok(v) = verb_rx.try_recv() {
                dev.send_verb(v);
                println!(" + Sent verb: {:08X}", v);
            }
            if let Some(r) = dev.read_response() {
                println!("Got response!");
                resp_tx.send(r);
            }
            thread::yield_now();
        }
    }
}
