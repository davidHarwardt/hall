use std::io;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use clap::Parser;
use rand::{prelude::*, distributions::Uniform};



#[derive(Debug, Clone, clap::Parser)]
struct Args {
    // number of total doors
    doors: usize,
    // number of doors to open by host
    open: Option<usize>,
    // wether to change the selected door
    change: bool,
}

impl Args {
    fn open(&self) -> usize { self.open.unwrap_or(self.doors - 2) }
}

#[inline]
fn play_single(args: &Args, rng: &mut ThreadRng, door_dist: &Uniform<usize>) -> bool {
    // select index of winning car
    let car_pos = rng.sample(door_dist);
    // select door for user
    let pick = rng.sample(door_dist);

    if args.change {
        let to_open = args.open();
        let mut opened = 0;
        let mut current = 0;
        while opened < to_open {
            if current == car_pos {
                // open the door as it is the first door not opened by the host
                return true;
            } else if current == pick {
                // do not open the user selected door
            } else { opened += 1 }

            current += 1;
        }
        // switch to the first door not opened by the host
        current == car_pos
    } else { car_pos == pick }
}

const PLAYS_PER_SEND: usize = 10000;

fn play(args: Arc<Args>, cancel: Arc<AtomicBool>, mut chan: bufchan::Sender<bool>) {
    let mut rng = rand::thread_rng();
    let dist = Uniform::new(0, args.doors);

    while !cancel.load(Ordering::Relaxed) {
        chan.send(play_single(&args, &mut rng, &dist))
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Arc::new(Args::parse());
    let cancel = Arc::new(AtomicBool::new(false));

    let (tx, rx) = bufchan::unbounded();

    tracing::info!("getting available parrallelism on system");
    let n_threads = std::thread::available_parallelism().unwrap_or_else(|_| {
        tracing::info!("could not get a value, using default parrallelism (16)");
        16.try_into().unwrap()
    });
    tracing::info!("preparing {n_threads} worker");

    let threads = (0..n_threads.get())
        .map(|i| {
            std::thread::Builder::new()
                .name(format!("worker-{i}"))
            .spawn({
                let args = Arc::clone(&args);
                let cancel = Arc::clone(&cancel);
                let chan = tx.clone();
                move || play(args, cancel, chan)
            })
        })
    .collect::<io::Result<Vec<_>>>()?;

    Ok(())
}

