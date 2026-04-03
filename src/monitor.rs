use crate::config::{Config, KillableApp};
use notify_rust::{Hint, Notification, Timeout, Urgency};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

pub type SharedRamMb = Arc<AtomicU64>;
pub type SharedTopProcs = Arc<Mutex<Vec<(String, u64)>>>;
pub type SharedPressure = Arc<AtomicU64>;

pub fn new_shared_ram() -> SharedRamMb {
    Arc::new(AtomicU64::new(0))
}

pub fn new_shared_top_procs() -> SharedTopProcs {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn new_shared_pressure() -> SharedPressure {
    Arc::new(AtomicU64::new(0))
}

pub fn pressure_from_shared(p: &SharedPressure) -> f64 {
    f64::from_bits(p.load(Ordering::Relaxed))
}

pub fn free_ram_mb(sys: &System) -> u64 {
    sys.available_memory() / 1_048_576
}

pub fn total_ram_mb(sys: &System) -> u64 {
    sys.total_memory() / 1_048_576
}

pub fn top_processes(sys: &System, n: usize) -> Vec<(String, u64)> {
    // Aggregate memory by process name (e.g. all "Web Content" workers sum together).
    // Skip threads: they share the parent's RSS, so including them inflates per-name totals.
    let mut by_name: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for (_, p) in sys.processes() {
        if p.thread_kind().is_some() {
            continue;
        }
        *by_name.entry(p.name().to_string()).or_insert(0) += p.memory();
    }
    let mut procs: Vec<(String, u64)> = by_name.into_iter().collect();
    procs.sort_by(|a, b| b.1.cmp(&a.1));
    procs.truncate(n);
    // Store raw bytes — callers format as needed
    procs
}

pub fn read_pressure_avg10() -> Option<f64> {
    let content = std::fs::read_to_string("/proc/pressure/memory").ok()?;
    for line in content.lines() {
        if line.starts_with("some ") {
            for part in line.split_whitespace() {
                if let Some(val) = part.strip_prefix("avg10=") {
                    return val.parse::<f64>().ok();
                }
            }
        }
    }
    None
}

fn find_all_matching(sys: &System, app_name: &str) -> Vec<Pid> {
    let name_lower = app_name.to_lowercase();
    sys.processes()
        .iter()
        .filter(|(_, p)| p.name().to_lowercase().contains(&name_lower))
        .map(|(pid, _)| *pid)
        .collect()
}

fn find_target<'a>(sys: &System, apps: &'a [KillableApp]) -> Option<(Vec<Pid>, &'a KillableApp)> {
    for app in apps {
        if !app.enabled {
            continue;
        }
        let pids = find_all_matching(sys, &app.name);
        if !pids.is_empty() {
            return Some((pids, app));
        }
    }
    None
}

fn notify_and_wait(app_label: &str, free_mb: u64, countdown_secs: u64) -> bool {
    let body = format!(
        "Free RAM: {free_mb} MB\nWill kill \"{app_label}\" in {countdown_secs}s to recover memory."
    );
    let (tx, rx) = mpsc::channel::<bool>();
    let app_label_owned = app_label.to_owned();

    thread::spawn(move || {
        let handle = match Notification::new()
            .summary("⚠️ Low Memory Warning")
            .body(&body)
            .icon("dialog-warning")
            .hint(Hint::SoundName("dialog-warning".to_owned()))
            .urgency(Urgency::Critical)
            .timeout(Timeout::Milliseconds((countdown_secs * 1000) as u32))
            .action("cancel", "Don't Kill")
            .show()
        {
            Ok(h) => h,
            Err(e) => {
                tracing::error!("Notification failed: {e}");
                let _ = tx.send(false);
                return;
            }
        };

        let notif_id = handle.id();
        let free_mb_clone = free_mb;
        let app_label_for_updater = app_label_owned.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_updater = Arc::clone(&stop);

        thread::spawn(move || {
            for remaining in (1..countdown_secs).rev() {
                thread::sleep(Duration::from_secs(1));
                if stop_updater.load(Ordering::Relaxed) {
                    break;
                }
                let updated_body = format!(
                    "Free RAM: {free_mb_clone} MB\nWill kill \"{app_label_for_updater}\" in {remaining}s to recover memory."
                );
                let _ = Notification::new()
                    .summary("⚠️ Low Memory Warning")
                    .body(&updated_body)
                    .icon("dialog-warning")
                    .urgency(Urgency::Critical)
                    .timeout(Timeout::Milliseconds((remaining * 1000) as u32))
                    .action("cancel", "Don't Kill")
                    .id(notif_id)
                    .show();
            }
        });

        handle.wait_for_action(|action| {
            let cancelled = action == "cancel";
            if cancelled {
                stop.store(true, Ordering::Relaxed);
                tracing::info!(app = %app_label_owned, "User cancelled kill");
            }
            let _ = tx.send(cancelled);
        });
    });

    match rx.recv_timeout(Duration::from_secs(countdown_secs + 2)) {
        Ok(cancelled) => cancelled,
        Err(_) => false,
    }
}

fn kill_process(sys: &mut System, pids: &[Pid], app_label: &str) {
    sys.refresh_processes();

    let mut term_sent = 0usize;
    for &pid in pids {
        if let Some(process) = sys.process(pid) {
            if process.kill_with(sysinfo::Signal::Term).unwrap_or(false) {
                term_sent += 1;
            }
        }
    }
    if term_sent > 0 {
        tracing::info!(count = term_sent, "Sent SIGTERM to {app_label}");
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut remaining: Vec<Pid> = pids.to_vec();
    while Instant::now() < deadline && !remaining.is_empty() {
        thread::sleep(Duration::from_millis(500));
        sys.refresh_processes();
        remaining.retain(|&pid| sys.process(pid).is_some());
    }

    let graceful_count = pids.len().saturating_sub(remaining.len());
    if graceful_count > 0 {
        tracing::info!(count = graceful_count, "{app_label} exited gracefully");
    }

    let mut force_killed = 0usize;
    for &pid in &remaining {
        if let Some(process) = sys.process(pid) {
            if process.kill() {
                force_killed += 1;
            }
        }
    }
    if force_killed > 0 {
        tracing::info!(count = force_killed, "Force-killed {app_label}");
    }

    let total = graceful_count + force_killed;
    if total > 0 {
        let _ = Notification::new()
            .summary("🔴 Process Killed")
            .body(&format!("Killed {total} \"{app_label}\" process(es) to recover memory."))
            .icon("dialog-warning")
            .hint(Hint::SoundName("complete".to_owned()))
            .urgency(Urgency::Normal)
            .timeout(Timeout::Milliseconds(5000))
            .show();
    } else {
        tracing::info!(app = app_label, "processes already gone");
    }
}

pub fn run(config: Arc<Mutex<Config>>, shared_ram: SharedRamMb, shared_pressure: SharedPressure) {
    let mut sys = System::new_all();
    let mut snooze_until: Option<Instant> = None;

    loop {
        sys.refresh_memory();
        sys.refresh_processes();

        let cfg = config.lock().unwrap().clone();
        let free_mb = free_ram_mb(&sys);
        shared_ram.store(free_mb, Ordering::Relaxed);

        let pressure_val = read_pressure_avg10().unwrap_or(0.0);
        shared_pressure.store(pressure_val.to_bits(), Ordering::Relaxed);

        let snoozing = snooze_until.map_or(false, |t| Instant::now() < t);

        let ram_trigger = free_mb < cfg.threshold_mb;
        let pressure_trigger =
            cfg.use_memory_pressure && pressure_val >= cfg.pressure_threshold_pct;

        if (ram_trigger || pressure_trigger) && !snoozing {
            if ram_trigger {
                tracing::info!(free_mb, threshold = cfg.threshold_mb, "Low RAM detected");
            }
            if pressure_trigger {
                tracing::info!(
                    pressure = pressure_val,
                    threshold = cfg.pressure_threshold_pct,
                    "Memory pressure detected"
                );
            }

            if let Some((pids, app)) = find_target(&sys, &cfg.killable_apps) {
                let app_label = app.label().to_owned();
                tracing::info!(count = pids.len(), "Target found: {app_label}");
                let cancelled = notify_and_wait(&app_label, free_mb, cfg.countdown_seconds);

                if cancelled {
                    snooze_until =
                        Some(Instant::now() + Duration::from_secs(cfg.snooze_seconds));
                    tracing::info!(seconds = cfg.snooze_seconds, "Snoozed: kill of {app_label} cancelled by user");
                } else {
                    kill_process(&mut sys, &pids, &app_label);
                    thread::sleep(Duration::from_secs(3));
                }
            } else {
                tracing::info!("Low RAM but no killable app is running");
            }
        }

        thread::sleep(Duration::from_secs(cfg.check_interval_seconds));
    }
}
