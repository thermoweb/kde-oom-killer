mod config;
mod monitor;
mod settings;
mod tray;

use clap::Parser;
use config::Config;
use monitor::new_shared_ram;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

/// Proactive memory-pressure killer for Linux desktops.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Kill when free RAM drops below this value (MB). Overrides config.
    #[arg(long, short = 't')]
    threshold: Option<u64>,

    /// Grace period before ramboing (seconds). Overrides config.
    #[arg(long, short = 'c')]
    countdown: Option<u64>,

    /// How often to poll memory (seconds). Overrides config.
    #[arg(long, short = 'i')]
    interval: Option<u64>,
}

fn main() {
    let args = Args::parse();

    println!("[rambo] Starting…");

    let mut cfg = Config::load();

    if let Some(t) = args.threshold {
        println!("[rambo] Threshold overridden via CLI: {t} MB");
        cfg.threshold_mb = t;
    }
    if let Some(c) = args.countdown {
        println!("[rambo] Countdown overridden via CLI: {c}s");
        cfg.countdown_seconds = c;
    }
    if let Some(i) = args.interval {
        println!("[rambo] Interval overridden via CLI: {i}s");
        cfg.check_interval_seconds = i;
    }

    let config = Arc::new(Mutex::new(cfg));
    let shared_ram = new_shared_ram();

    let total_ram_mb = {
        use sysinfo::System;
        let sys = System::new_all();
        monitor::total_ram_mb(&sys)
    };

    // Channel: tray sends () to ask the main thread to open the settings window.
    let (settings_tx, settings_rx) = mpsc::channel::<()>();

    // Tray + RAM refresh run in background threads.
    let tray_handle = tray::start(
        Arc::clone(&shared_ram),
        Arc::clone(&config),
        total_ram_mb,
        settings_tx,
    );

    {
        let shared_ram_tray = Arc::clone(&shared_ram);
        std::thread::spawn(move || {
            use sysinfo::System;
            let mut sys = System::new_all();
            loop {
                sys.refresh_memory();
                let free_mb = monitor::free_ram_mb(&sys);
                shared_ram_tray.store(free_mb, Ordering::Relaxed);
                tray_handle.notify();
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }

    // Monitor runs in a background thread.
    {
        let config = Arc::clone(&config);
        let shared_ram = Arc::clone(&shared_ram);
        std::thread::spawn(move || monitor::run(config, shared_ram));
    }

    // Main thread: block waiting for settings-open requests, then run the window.
    // eframe/winit requires the event loop on the main thread.
    for () in settings_rx {
        settings::open_blocking(Arc::clone(&config));
    }
}
