use crate::config::Config;
use eframe::egui;
use egui_plot::{HLine, Line, LineStyle, Plot, PlotPoints, VLine};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub const MAX_SAMPLES: usize = 900; // ~30 min at 2 s intervals
pub const MAX_LOG_LINES: usize = 200;

static APP_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Seconds elapsed since the first call (i.e. app start). Used as the X axis for charts.
pub fn elapsed_secs() -> f64 {
    APP_START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

// ── Shared types ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct HistorySample {
    pub elapsed_secs: f64,
    pub free_mb: f64,
    pub pressure: f64,
}

#[derive(Clone)]
pub struct KillEvent {
    pub elapsed_secs: f64,
    pub app_name: String,
}

pub type SharedHistory = Arc<Mutex<VecDeque<HistorySample>>>;
pub type SharedKillEvents = Arc<Mutex<Vec<KillEvent>>>;
pub type SharedLogLines = Arc<Mutex<VecDeque<String>>>;

pub fn new_shared_history() -> SharedHistory {
    Arc::new(Mutex::new(VecDeque::with_capacity(MAX_SAMPLES + 1)))
}

pub fn new_shared_kill_events() -> SharedKillEvents {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn new_shared_log_lines() -> SharedLogLines {
    Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES + 1)))
}

// ── Push helpers ─────────────────────────────────────────────────────────────

pub fn push_sample(history: &SharedHistory, free_mb: u64, pressure: f64) {
    let elapsed = elapsed_secs();
    let mut h = history.lock().unwrap();
    if h.len() >= MAX_SAMPLES {
        h.pop_front();
    }
    h.push_back(HistorySample {
        elapsed_secs: elapsed,
        free_mb: free_mb as f64,
        pressure,
    });
}

pub fn push_kill_event(kill_events: &SharedKillEvents, app_name: String) {
    kill_events.lock().unwrap().push(KillEvent {
        elapsed_secs: elapsed_secs(),
        app_name,
    });
}

pub fn push_log_line(log_lines: &SharedLogLines, line: String) {
    let mut l = log_lines.lock().unwrap();
    if l.len() >= MAX_LOG_LINES {
        l.pop_front();
    }
    l.push_back(line);
}

// ── History window ───────────────────────────────────────────────────────────

/// Opens the history window. Blocks until the window is closed.
/// Must be called from the main thread (winit/eframe requirement).
pub fn open_blocking(
    shared_history: SharedHistory,
    shared_kill_events: SharedKillEvents,
    shared_log_lines: SharedLogLines,
    config: Arc<Mutex<Config>>,
    total_ram_mb: u64,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rambo — History")
            .with_inner_size([720.0, 640.0])
            .with_resizable(true),
        ..Default::default()
    };

    let _ = eframe::run_simple_native("rambo-history", options, move |ctx, _frame| {
        ctx.set_pixels_per_point(1.2);
        ctx.request_repaint_after(std::time::Duration::from_secs(2));

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("📊 Rambo — History");
                ui.add_space(8.0);

                let threshold_mb = config.lock().unwrap().threshold_mb as f64;
                let use_pressure = config.lock().unwrap().use_memory_pressure;
                let pressure_threshold = config.lock().unwrap().pressure_threshold_pct;

                let history: Vec<HistorySample> =
                    shared_history.lock().unwrap().iter().cloned().collect();
                let kill_events: Vec<KillEvent> =
                    shared_kill_events.lock().unwrap().clone();

                // ── RAM chart ───────────────────────────────────────────────
                ui.label(egui::RichText::new("Free RAM (MB)").strong());

                let ram_points: PlotPoints =
                    history.iter().map(|s| [s.elapsed_secs, s.free_mb]).collect();

                Plot::new("ram_chart")
                    .height(180.0)
                    .include_y(0.0)
                    .include_y(total_ram_mb as f64)
                    .y_axis_label("MB")
                    .show(ui, |plot_ui| {
                        // Threshold dashed line
                        plot_ui.hline(
                            HLine::new("Threshold", threshold_mb)
                                .color(egui::Color32::from_rgb(220, 80, 80))
                                .style(LineStyle::Dashed { length: 8.0 }),
                        );
                        // Kill event vertical markers
                        for ev in &kill_events {
                            plot_ui.vline(
                                VLine::new(format!("💀 {}", ev.app_name), ev.elapsed_secs)
                                    .color(egui::Color32::from_rgb(230, 60, 60)),
                            );
                        }
                        // RAM line
                        plot_ui.line(
                            Line::new("Free RAM", ram_points)
                                .color(egui::Color32::from_rgb(60, 200, 100)),
                        );
                    });

                ui.add_space(8.0);

                // ── Pressure chart ──────────────────────────────────────────
                ui.label(egui::RichText::new("Memory Pressure (%)").strong());

                if use_pressure {
                    let pressure_points: PlotPoints = history
                        .iter()
                        .map(|s| [s.elapsed_secs, s.pressure])
                        .collect();

                    Plot::new("pressure_chart")
                        .height(120.0)
                        .include_y(0.0)
                        .include_y(100.0)
                        .y_axis_label("%")
                        .show(ui, |plot_ui| {
                            plot_ui.hline(
                                HLine::new("Threshold", pressure_threshold)
                                    .color(egui::Color32::from_rgb(220, 80, 80))
                                    .style(LineStyle::Dashed { length: 8.0 }),
                            );
                            for ev in &kill_events {
                                plot_ui.vline(
                                    VLine::new(format!("💀 {}", ev.app_name), ev.elapsed_secs)
                                        .color(egui::Color32::from_rgb(230, 60, 60)),
                                );
                            }
                            plot_ui.line(
                                Line::new("Pressure", pressure_points)
                                    .color(egui::Color32::from_rgb(100, 150, 230)),
                            );
                        });
                } else {
                    ui.label(
                        egui::RichText::new("Memory pressure monitoring is disabled.")
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // ── Log panel ───────────────────────────────────────────────
                ui.label(
                    egui::RichText::new(format!("Recent Logs (last {MAX_LOG_LINES} lines)"))
                        .strong(),
                );
                ui.add_space(4.0);

                let log_lines: Vec<String> =
                    shared_log_lines.lock().unwrap().iter().cloned().collect();

                egui::ScrollArea::vertical()
                    .id_salt("log_scroll")
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &log_lines {
                            ui.label(egui::RichText::new(line).monospace().size(11.0));
                        }
                    });

                ui.add_space(8.0);
            });
        });
    });
}
