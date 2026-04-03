use crate::config::{Config, KillableApp};
use crate::monitor::{SharedPressure, SharedRamMb, pressure_from_shared};
use eframe::egui;
use egui::{CentralPanel, ScrollArea};
use egui_dnd::dnd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Opens the settings window. Blocks until the window is closed.
/// Must be called from the main thread (winit/eframe requirement).
pub fn open_blocking(
    config: Arc<Mutex<Config>>,
    shared_ram: SharedRamMb,
    total_ram_mb: u64,
    shared_pressure: SharedPressure,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rambo — Settings")
            .with_inner_size([480.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };

    let new_name = Arc::new(Mutex::new(String::new()));
    let new_display = Arc::new(Mutex::new(String::new()));
    let test_mode_active = Arc::new(AtomicBool::new(false));
    let saved_threshold = Arc::new(AtomicU64::new(0));

    let _ = eframe::run_simple_native("rambo-settings", options, move |ctx, _frame| {
        ctx.set_pixels_per_point(1.2);
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("⚙ Rambo Settings");
                ui.add_space(8.0);

                // Live RAM progress bar
                let free_mb = shared_ram.load(Ordering::Relaxed);
                let ram_ratio = if total_ram_mb > 0 {
                    (free_mb as f32 / total_ram_mb as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let threshold = config.lock().unwrap().threshold_mb;
                let bar_color = if free_mb < threshold {
                    egui::Color32::from_rgb(220, 50, 50)
                } else if free_mb < threshold.saturating_mul(2) {
                    egui::Color32::from_rgb(230, 160, 0)
                } else {
                    egui::Color32::from_rgb(60, 180, 60)
                };
                ui.add(
                    egui::ProgressBar::new(ram_ratio)
                        .text(format!("Free RAM: {free_mb} MB / {total_ram_mb} MB"))
                        .fill(bar_color),
                );

                // Memory pressure bar (when enabled)
                let use_pressure = config.lock().unwrap().use_memory_pressure;
                if use_pressure {
                    let pressure = pressure_from_shared(&shared_pressure);
                    let pressure_threshold = config.lock().unwrap().pressure_threshold_pct;
                    let pressure_ratio = (pressure as f32 / 100.0).clamp(0.0, 1.0);
                    let pressure_color = if pressure >= pressure_threshold {
                        egui::Color32::from_rgb(220, 50, 50)
                    } else {
                        egui::Color32::from_rgb(60, 180, 60)
                    };
                    ui.add(
                        egui::ProgressBar::new(pressure_ratio)
                            .text(format!("Memory Pressure: {pressure:.1}%"))
                            .fill(pressure_color),
                    );
                }

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("General").strong());
                    ui.add_space(4.0);

                    // Snapshot the config briefly so the monitor thread is never blocked
                    // during the entire frame render.
                    let mut local = config.lock().unwrap().clone();
                    let mut changed = false;

                    egui::Grid::new("general_grid")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("RAM threshold (MB)");
                            changed |= ui.add(egui::DragValue::new(&mut local.threshold_mb).range(64..=65536)).changed();
                            ui.end_row();
                            ui.label("Countdown (seconds)");
                            changed |= ui.add(egui::DragValue::new(&mut local.countdown_seconds).range(5..=300)).changed();
                            ui.end_row();
                            ui.label("Check interval (seconds)");
                            changed |= ui.add(egui::DragValue::new(&mut local.check_interval_seconds).range(1..=60)).changed();
                            ui.end_row();
                            ui.label("Snooze duration (seconds)");
                            changed |= ui.add(egui::DragValue::new(&mut local.snooze_seconds).range(30..=3600)).changed();
                            ui.end_row();
                            ui.label("Enable memory pressure");
                            changed |= ui.checkbox(&mut local.use_memory_pressure, "").changed();
                            ui.end_row();
                            if local.use_memory_pressure {
                                ui.label("Pressure threshold (%)");
                                changed |= ui.add(egui::DragValue::new(&mut local.pressure_threshold_pct).range(1.0..=100.0).speed(0.5)).changed();
                                ui.end_row();
                            }
                        });

                    if changed {
                        let mut cfg = config.lock().unwrap();
                        *cfg = local;
                        let _ = cfg.save();
                    }
                });

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("Kill Priority").strong());
                    ui.label(
                        egui::RichText::new("Drag ≡ to reorder. First enabled running app is targeted.")
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(6.0);

                    let mut apps = config.lock().unwrap().killable_apps.clone();
                    let mut changed = false;
                    let mut to_remove: Option<usize> = None;

                    let mut row = 0usize;
                    let dnd_response = dnd(ui, "kill_priority_dnd").show_vec(&mut apps, |ui, app, handle, state| {
                        let current = row;
                        row += 1;
                        ui.horizontal(|ui| {
                            handle.ui(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("≡")
                                        .color(if state.dragged {
                                            ui.visuals().strong_text_color()
                                        } else {
                                            ui.visuals().weak_text_color()
                                        }),
                                );
                            });
                            changed |= ui.checkbox(&mut app.enabled, "").changed();
                            ui.label(app.label());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                    to_remove = Some(current);
                                    changed = true;
                                }
                            });
                        });
                    });

                    // Persist after a completed drag-and-drop reorder
                    if dnd_response.is_drag_finished() {
                        changed = true;
                    }

                    if let Some(idx) = to_remove {
                        apps.remove(idx);
                    }

                    if changed {
                        let mut cfg = config.lock().unwrap();
                        cfg.killable_apps = apps;
                        let _ = cfg.save();
                    }
                });

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("Add App").strong());
                    ui.add_space(4.0);

                    let mut name = new_name.lock().unwrap();
                    let mut display = new_display.lock().unwrap();

                    egui::Grid::new("add_app_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Process name");
                            ui.add(egui::TextEdit::singleline(&mut *name).hint_text("e.g. spotify"));
                            ui.end_row();
                            ui.label("Display name");
                            ui.add(egui::TextEdit::singleline(&mut *display).hint_text("e.g. Spotify (optional)"));
                            ui.end_row();
                        });

                    ui.add_space(4.0);

                    let can_add = !name.trim().is_empty();
                    if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                        let display_name = if display.trim().is_empty() {
                            None
                        } else {
                            Some(display.trim().to_owned())
                        };
                        let new_app = KillableApp {
                            name: name.trim().to_lowercase(),
                            display_name,
                            enabled: true,
                        };
                        let mut cfg = config.lock().unwrap();
                        cfg.killable_apps.push(new_app);
                        let _ = cfg.save();
                        drop(cfg);
                        name.clear();
                        display.clear();
                    }
                });

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("Test Mode").strong());
                    ui.add_space(4.0);

                    let is_active = test_mode_active.load(Ordering::Relaxed);
                    if is_active {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 50, 50),
                            "⚠ Test mode active — threshold set to maximum",
                        );
                        ui.add_space(4.0);
                        if ui.button("Deactivate").clicked() {
                            let old = saved_threshold.load(Ordering::Relaxed);
                            config.lock().unwrap().threshold_mb = old;
                            test_mode_active.store(false, Ordering::Relaxed);
                        }
                    } else {
                        ui.label("Temporarily set threshold to maximum to trigger kill logic.");
                        ui.add_space(4.0);
                        if ui.button("Test Mode").clicked() {
                            let current = config.lock().unwrap().threshold_mb;
                            saved_threshold.store(current, Ordering::Relaxed);
                            config.lock().unwrap().threshold_mb = u64::MAX;
                            test_mode_active.store(true, Ordering::Relaxed);
                        }
                    }
                });

                ui.add_space(8.0);
            });
        });
    });
}
