use std::io::{self, Write};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

use clap::Parser;
use rand::{prelude::*, distributions::Uniform};



#[derive(Debug, Clone, clap::Parser)]
struct Args {
    #[arg(short, long)]
    /// number of total doors
    doors: usize,

    #[arg(short, long)]
    /// number of doors to open by host
    open: Option<usize>,

    #[arg(short, long)]
    /// wether to change the selected door
    change: bool,

    #[arg(short, long)]
    /// if set automaticly terminates after the 
    /// specified number of seconds
    run_for_seconds: Option<usize>,
}

impl Args {
    fn open(&self) -> usize { self.open.unwrap_or(self.doors - 2) }
}

#[inline]
fn play_single(args: &Args, doors: &mut [bool], rng: &mut ThreadRng, door_dist: &Uniform<usize>) -> bool {
    // select index of winning car
    let car_pos = rng.sample(door_dist);
    // select door for user
    let pick = rng.sample(door_dist);

    doors.fill(false);

    if args.change {
        let to_open = args.open();
        let mut opened = 0;
        let mut current = 0;
        while opened < to_open {
            if current != car_pos && current != pick {
                doors[current] = true;
                // open the door 
                opened += 1;
            }

            current = current + 1;
        }
        let to_open = doors.iter().copied().enumerate().position(|(i, v)| !v && i != pick).unwrap();
        car_pos == to_open
    } else { car_pos == pick }
}

fn play(args: Arc<Args>, cancel: Arc<AtomicBool>, mut chan: bufchan::Sender<bool>) {
    let mut rng = rand::thread_rng();
    let dist = Uniform::new(0, args.doors);
    let mut doors = vec![false; args.doors];

    while !cancel.load(Ordering::Relaxed) {
        chan.send(play_single(&args, &mut doors, &mut rng, &dist))
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Arc::new(Args::parse());
    let cancel = Arc::new(AtomicBool::new(false));

    let (tx, mut rx) = bufchan::unbounded();

    tracing::info!("getting available parrallelism on system");
    let n_threads = std::thread::available_parallelism().unwrap_or_else(|_| {
        tracing::info!("could not get a value, using default parrallelism (16)");
        16.try_into().unwrap()
    });
    tracing::info!("preparing {n_threads} worker threads");

    tracing::info!("running the game");
    tracing::info!("total doors: {}", args.doors);
    tracing::info!("opening {} door(s)", args.open());
    tracing::info!("changing doors: {}", args.change);

    let threads = (0..n_threads.get())
        .map(|i| {
            std::thread::Builder::new()
                .name(format!("worker-{i}"))
            .spawn({
                let args = Arc::clone(&args);
                let cancel = Arc::clone(&cancel);
                let chan = tx.clone();
                move || {
                    play(args, cancel, chan);
                    tracing::info!("worker-{i} finished");
                }
            })
        })
    .collect::<io::Result<Vec<_>>>()?;

    drop(tx);

    let collector_thread = std::thread::spawn({
        move || {
            println!();
            let mut total = 0usize;
            let mut wins = 0usize;
            let mut start = Instant::now();

            while let Some(won) = rx.recv() {
                wins += won as usize;
                total += 1;
                if start.elapsed().as_secs_f32() > 1.0 {
                    start = Instant::now();
                    print!("\rrunning: {wins} wins ({total} games)");
                    std::io::stdout().lock().flush().expect("could not flush");
                }
            }
            println!();
            tracing::info!("collector finshed");
            (wins, total)
        }
    });

    if let Some(v) = args.run_for_seconds {
        tracing::info!("running for {v} seconds");
        std::thread::sleep(Duration::from_secs(v as _));
    } else {
        tracing::info!("press enter to stop counting");
        std::io::stdin().read_line(&mut String::new())?;
    }

    cancel.store(true, Ordering::Relaxed);
    for t in threads { t.join().expect("got an error joining a worker thread") }
    let (wins, total) = collector_thread.join().expect("got an error joining the collector thread");

    tracing::info!("won {wins} times (out of {total} games): {:.2}%", (wins as f64 / total as f64) * 100.0);

    Ok(())
}

