mod config;
mod history;
mod log_capture;
mod monitor;
mod settings;
mod tray;

use clap::Parser;
use config::Config;
use history::{new_shared_history, new_shared_kill_events, new_shared_log_lines, push_sample};
use log_capture::LogCaptureLayer;
use monitor::{new_shared_pressure, new_shared_ram, new_shared_top_procs};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray::WindowRequest;

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

fn init_tracing(log_lines: history::SharedLogLines) {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // The in-app log panel only shows rambo's own logs, not third-party noise.
    let capture = LogCaptureLayer { log_lines };

    match tracing_journald::layer() {
        Ok(journald_layer) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(journald_layer)
                .with(capture)
                .init();
        }
        Err(_) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .with(capture)
                .init();
        }
    }
}

fn main() {
    let shared_log_lines = new_shared_log_lines();
    init_tracing(Arc::clone(&shared_log_lines));
    let args = Args::parse();

    tracing::info!("Starting…");

    let mut cfg = Config::load();

    if let Some(t) = args.threshold {
        tracing::info!(threshold = t, "Threshold overridden via CLI");
        cfg.threshold_mb = t;
    }
    if let Some(c) = args.countdown {
        tracing::info!(countdown = c, "Countdown overridden via CLI");
        cfg.countdown_seconds = c;
    }
    if let Some(i) = args.interval {
        tracing::info!(interval = i, "Interval overridden via CLI");
        cfg.check_interval_seconds = i;
    }

    let config = Arc::new(Mutex::new(cfg));
    let shared_ram = new_shared_ram();
    let shared_top_procs = new_shared_top_procs();
    let shared_pressure = new_shared_pressure();
    let shared_history = new_shared_history();
    let shared_kill_events = new_shared_kill_events();

    let total_ram_mb = {
        use sysinfo::System;
        let sys = System::new_all();
        monitor::total_ram_mb(&sys)
    };

    // Channel: tray sends WindowRequest to ask the main thread to open a window.
    let (window_tx, window_rx) = mpsc::channel::<WindowRequest>();

    // Tray + RAM refresh run in background threads.
    let tray_handle = tray::start(
        Arc::clone(&shared_ram),
        Arc::clone(&config),
        total_ram_mb,
        window_tx,
        Arc::clone(&shared_top_procs),
        Arc::clone(&shared_pressure),
    );

    {
        let shared_ram_tray = shared_ram.clone();
        let shared_top_procs_tray = shared_top_procs.clone();
        let shared_pressure_tray = shared_pressure.clone();
        let shared_history_tray = shared_history.clone();
        std::thread::spawn(move || {
            use sysinfo::System;
            let mut sys = System::new_all();
            loop {
                sys.refresh_memory();
                sys.refresh_processes();
                let free_mb = monitor::free_ram_mb(&sys);
                shared_ram_tray.store(free_mb, Ordering::Relaxed);

                let tops = monitor::top_processes(&sys, 10);
                *shared_top_procs_tray.lock().unwrap() = tops;

                let pressure = monitor::read_pressure_avg10().unwrap_or(0.0);
                shared_pressure_tray.store(pressure.to_bits(), Ordering::Relaxed);

                push_sample(&shared_history_tray, free_mb, pressure);

                tray_handle.notify();
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }

    // Monitor runs in a background thread.
    {
        let config = Arc::clone(&config);
        let shared_ram = Arc::clone(&shared_ram);
        let shared_pressure = Arc::clone(&shared_pressure);
        let shared_kill_events = Arc::clone(&shared_kill_events);
        std::thread::spawn(move || monitor::run(config, shared_ram, shared_pressure, shared_kill_events));
    }

    // Main thread: block waiting for window-open requests, then run the window.
    // eframe/winit requires the event loop on the main thread.
    for req in window_rx {
        match req {
            WindowRequest::Settings => settings::open_blocking(
                Arc::clone(&config),
                Arc::clone(&shared_ram),
                total_ram_mb,
                Arc::clone(&shared_pressure),
            ),
            WindowRequest::History => history::open_blocking(
                Arc::clone(&shared_history),
                Arc::clone(&shared_kill_events),
                Arc::clone(&shared_log_lines),
                Arc::clone(&config),
                total_ram_mb,
            ),
        }
    }
}
