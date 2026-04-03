use crate::config::{Config, KillableApp};
use notify_rust::{Notification, Timeout, Urgency};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

/// Shared free-RAM counter updated by the monitor loop (read by the tray).
pub type SharedRamMb = Arc<AtomicU64>;

pub fn new_shared_ram() -> SharedRamMb {
    Arc::new(AtomicU64::new(0))
}

/// Returns free RAM in MB.
pub fn free_ram_mb(sys: &System) -> u64 {
    sys.available_memory() / 1_048_576
}

/// Returns total RAM in MB.
pub fn total_ram_mb(sys: &System) -> u64 {
    sys.total_memory() / 1_048_576
}

/// Returns all PIDs matching the given app name.
fn find_all_matching(sys: &System, app_name: &str) -> Vec<Pid> {
    let name_lower = app_name.to_lowercase();
    sys.processes()
        .iter()
        .filter(|(_, p)| p.name().to_lowercase().contains(&name_lower))
        .map(|(pid, _)| *pid)
        .collect()
}

/// Returns the first app from the priority list that has at least one running process.
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

/// Sends a critical notification with a "Don't Kill" action.
/// Returns true if the user cancelled, false if countdown expired.
fn notify_and_wait(app_label: &str, free_mb: u64, countdown_secs: u64) -> bool {
    let body = format!(
        "Free RAM: {free_mb} MB\nWill kill \"{app_label}\" in {countdown_secs}s to recover memory."
    );

    let (tx, rx) = mpsc::channel::<bool>();
    let body_clone = body.clone();
    let app_label_owned = app_label.to_owned();

    thread::spawn(move || {
        match Notification::new()
            .summary("⚠️ Low Memory Warning")
            .body(&body_clone)
            .icon("dialog-warning")
            .urgency(Urgency::Critical)
            .timeout(Timeout::Milliseconds((countdown_secs * 1000) as u32))
            .action("cancel", "Don't Kill")
            .show()
        {
            Ok(handle) => {
                handle.wait_for_action(|action| {
                    let cancelled = action == "cancel";
                    if cancelled {
                        println!("[rambo] User cancelled kill of \"{app_label_owned}\"");
                    }
                    let _ = tx.send(cancelled);
                });
            }
            Err(e) => {
                eprintln!("[rambo] Notification failed: {e}");
                let _ = tx.send(false);
            }
        }
    });

    match rx.recv_timeout(Duration::from_secs(countdown_secs + 2)) {
        Ok(cancelled) => cancelled,
        Err(_) => false, // timeout → not cancelled
    }
}

/// Kills all processes in the list and sends a follow-up notification.
fn kill_process(sys: &mut System, pids: &[Pid], app_label: &str) {
    sys.refresh_processes();
    let mut killed = 0;
    for &pid in pids {
        if let Some(process) = sys.process(pid) {
            if process.kill() {
                killed += 1;
            }
        }
    }
    if killed > 0 {
        println!("[rambo] Killed {killed} \"{app_label}\" process(es)");
        let _ = Notification::new()
            .summary("🔴 Process Killed")
            .body(&format!("Killed {killed} \"{app_label}\" process(es) to recover memory."))
            .icon("dialog-warning")
            .urgency(Urgency::Normal)
            .timeout(Timeout::Milliseconds(5000))
            .show();
    } else {
        println!("[rambo] \"{app_label}\" processes already gone");
    }
}

/// Main monitor loop. Runs forever, polling memory and acting when low.
pub fn run(config: Arc<Mutex<Config>>, shared_ram: SharedRamMb) {
    let mut sys = System::new_all();
    let mut snooze_until: Option<Instant> = None;

    loop {
        sys.refresh_memory();
        sys.refresh_processes();

        let cfg = config.lock().unwrap().clone();
        let free_mb = free_ram_mb(&sys);

        shared_ram.store(free_mb, Ordering::Relaxed);

        let snoozing = snooze_until.map_or(false, |t| Instant::now() < t);

        if free_mb < cfg.threshold_mb && !snoozing {
            println!("[rambo] Low RAM: {free_mb} MB free (threshold: {} MB)", cfg.threshold_mb);

            if let Some((pids, app)) = find_target(&sys, &cfg.killable_apps) {
                let app_label = app.label().to_owned();
                println!("[rambo] Target: \"{}\" ({} process(es))", app_label, pids.len());
                let cancelled = notify_and_wait(&app_label, free_mb, cfg.countdown_seconds);

                if cancelled {
                    snooze_until = Some(Instant::now() + Duration::from_secs(cfg.snooze_seconds));
                    println!(
                        "[rambo] Snoozed for {} seconds",
                        cfg.snooze_seconds
                    );
                } else {
                    kill_process(&mut sys, &pids, &app_label);
                    // Let memory settle before next check
                    thread::sleep(Duration::from_secs(3));
                }
            } else {
                println!("[rambo] Low RAM but no killable app is running");
            }
        }

        thread::sleep(Duration::from_secs(cfg.check_interval_seconds));
    }
}
