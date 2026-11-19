//! F-34: the updater's card (top right, never takes the keyboard) and Help ▸ Check for Updates..., as in the
//! approved mockups (design/F-34 and design/F-34/auto). The work itself is in `update.rs`.
use crate::app::{App, Then};
use crate::kit;
use crate::theme;
use crate::update::{self, Check, CheckJob, DownloadJob, Release, Staged, UpdateError};
use egui::{pos2, vec2, Align2, Color32, Rect, Sense, Stroke, Ui};

pub enum Stage {
    Idle,
    Checking,
    Available(Release),
    /// (release, the job; or a fixed progress for the screenshots: bytes, total, seconds left)
    Downloading(Release, Option<DownloadJob>, (u64, u64, f64)),
    Ready(Release, Option<Staged>),
    Failed(Release, UpdateError),
    Latest,
    Offline,
}

pub struct Updater {
    pub stage: Stage,
    check: Option<CheckJob>,
    /// Help ▸ Check for Updates...: the answer shows as a dialog in the middle (latest, offline, available)
    pub manual: bool,
    /// the card is showing (× and Later hide it; a download goes on)
    pub card: bool,
    started: bool,
    /// "Updated to …" until this time (the frame clock, seconds)
    pub toast_until: Option<f64>,
    /// the update was handed to the installer (once)
    pub installed: bool,
    /// Restart Now went through the unsaved-changes question
    pub restart_now: bool,
    /// when the last check finished (seconds since 1970)
    checked_at: Option<u64>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater { stage: Stage::Idle, check: None, manual: false, card: false, started: false, toast_until: None, installed: false, restart_now: false, checked_at: None }
    }
}

fn now_secs() -> u64 {
    crate::clock::unix_secs()
}

/// Opens a web page in the browser.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn mb(b: u64) -> String {
    let m = b as f64 / 1e6;
    if m < 10.0 {
        format!("{m:.1}")
    } else {
        format!("{m:.0}")
    }
}

fn left_text(s: f64) -> String {
    if s < 60.0 {
        format!("about {} s left", ((s / 5.0).ceil() * 5.0).max(5.0) as u64)
    } else {
        format!("about {} min left", (s / 60.0).ceil() as u64)
    }
}

impl App {
    /// Every frame (Mac and Windows): starts the daily check after the settings are read, follows the check and the
    /// download, and draws the card, the dialog and the "Updated to …" note.
    pub fn updater_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.updater.started && !self.screenshot_mode && self.midi.is_some() {
            self.updater.started = true;
            // the version an update installed says so once
            if let Some(v) = self.prefs.update_installed.take() {
                if v != update::VERSION {
                    self.updater.toast_until = Some(now + 6.0);
                }
                crate::prefs::save(&self.prefs);
            }
            // each time Fermata opens; the card only shows when there is a newer version
            if self.prefs.check_updates {
                self.start_update_check(false);
            }
        }
        self.poll_update();
        if self.updater.check.is_some() || matches!(self.updater.stage, Stage::Downloading(..)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if self.updater.restart_now {
            self.updater.restart_now = false;
            self.install_update(true);
        }
        if self.updater.manual {
            self.update_dialog(ctx);
        } else if self.updater.card {
            self.update_card(ctx);
        }
        if let Some(t) = self.updater.toast_until {
            if now > t {
                self.updater.toast_until = None;
            } else {
                self.updated_toast(ctx);
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }

    /// Looks for a new version (`manual`: Help ▸ Check for Updates..., which answers in a dialog).
    pub fn start_update_check(&mut self, manual: bool) {
        if matches!(self.updater.stage, Stage::Downloading(..) | Stage::Ready(..)) {
            // an update is already on its way: show it
            self.updater.card = true;
            self.updater.manual = false;
            return;
        }
        self.updater.manual = manual;
        if manual {
            self.updater.stage = Stage::Checking;
        }
        self.prefs.last_update_check = now_secs();
        crate::prefs::save(&self.prefs);
        self.updater.check = Some(CheckJob::start());
    }

    fn poll_update(&mut self) {
        if let Some(r) = self.updater.check.as_ref().and_then(|j| j.try_result()) {
            self.updater.check = None;
            let manual = self.updater.manual;
            match r {
                Ok(Check::Newer(rel)) => {
                    self.updater.checked_at = Some(now_secs());
                    // a check when Fermata opens stays quiet about a skipped version
                    if manual || self.prefs.skipped_version.as_deref() != Some(rel.version.as_str()) {
                        self.updater.stage = Stage::Available(rel);
                        self.updater.card = !manual;
                    }
                }
                Ok(Check::Latest(_)) => {
                    self.updater.checked_at = Some(now_secs());
                    self.updater.stage = if manual { Stage::Latest } else { Stage::Idle };
                }
                // no connection when Fermata opens: nothing at all
                Err(_) => self.updater.stage = if manual { Stage::Offline } else { Stage::Idle },
            }
        }
        let mut next = None;
        if let Stage::Downloading(rel, Some(job), _) = &self.updater.stage {
            if let Some(r) = job.try_result() {
                next = Some(match r.and_then(|zip| update::target().and_then(|t| update::stage(&zip, &t))) {
                    Ok(staged) => Stage::Ready(rel.clone(), Some(staged)),
                    Err(UpdateError::Cancelled) => Stage::Available(rel.clone()),
                    Err(e) => Stage::Failed(rel.clone(), e),
                });
            }
        }
        if let Some(s) = next {
            let cancelled = matches!(s, Stage::Available(_));
            self.updater.stage = s;
            if cancelled {
                self.updater.card = false;
                self.status = "The update was cancelled.".into();
            } else {
                self.updater.card = true;
            }
        }
    }

    /// Update: checks where Fermata runs first (a copy inside the .dmg can't replace itself), then downloads.
    fn begin_update(&mut self, rel: Release) {
        self.updater.manual = false;
        self.updater.card = true;
        if let Err(e) = update::target() {
            self.updater.stage = Stage::Failed(rel, e);
            return;
        }
        let job = DownloadJob::start(&rel);
        self.updater.stage = Stage::Downloading(rel, Some(job), (0, 0, 0.0));
    }

    /// Hands the unpacked update to the installer. `reopen`: Restart Now (Fermata quits and the new one opens);
    /// otherwise the update is put in place as Fermata closes.
    pub fn install_update(&mut self, reopen: bool) {
        if self.updater.installed {
            return;
        }
        let Stage::Ready(rel, Some(staged)) = &self.updater.stage else { return };
        let rel = rel.clone();
        match update::install(staged, reopen) {
            Ok(()) => {
                self.updater.installed = true;
                self.prefs.update_installed = Some(update::VERSION.to_string());
                crate::prefs::save(&self.prefs);
                if reopen {
                    self.dirty = false;
                    self.quit_requested = true;
                }
            }
            Err(e) => {
                self.updater.stage = Stage::Failed(rel, e);
                self.updater.card = true;
            }
        }
    }

    /// Closing Fermata with an update ready ("Later"): it is installed now, without opening again.
    pub fn install_update_on_exit(&mut self) {
        if matches!(self.updater.stage, Stage::Ready(_, Some(_))) {
            self.install_update(false);
        }
    }

    /// "Updating… 46%" in the status bar while the card is hidden.
    pub fn update_status(&self) -> Option<String> {
        match &self.updater.stage {
            Stage::Downloading(_, job, demo) if !self.updater.card => {
                let (d, t) = job.as_ref().map(|j| j.progress()).unwrap_or((demo.0, demo.1));
                Some(match (d * 100).checked_div(t) {
                    Some(p) => format!("Updating… {p}%"),
                    None => "Updating…".into(),
                })
            }
            _ => None,
        }
    }

    // ---------------- drawing ----------------

    fn update_card(&mut self, ctx: &egui::Context) {
        let k = theme::ctx_tokens(ctx);
        let pos = pos2(ctx.content_rect().right() - 16.0, 84.0);
        egui::Area::new(egui::Id::unique("update-card"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(pos)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(k.panel)
                    .stroke(Stroke::new(1.0, k.line))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 14 })
                    .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(40) })
                    .show(ui, |ui| {
                        ui.set_width(372.0);
                        self.card_body(ui, false);
                    });
            });
    }

    fn update_dialog(&mut self, ctx: &egui::Context) {
        let k = theme::ctx_tokens(ctx);
        // the score dims behind the dialog
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("update-dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(40));
        });
        egui::Window::new("Check for Updates")
            .id(egui::Id::unique("update-dialog"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 110.0))
            .frame(kit::window_frame(ctx).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                ui.set_width(372.0);
                let _ = k;
                self.card_body(ui, true);
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.updater.manual = false;
        }
    }

    fn checkbox(&mut self, ui: &mut Ui) {
        let mut on = self.prefs.check_updates;
        if kit::checkbox(ui, &mut on, "Check for updates when Fermata opens").changed() {
            self.prefs.check_updates = on;
            crate::prefs::save(&self.prefs);
        }
    }

    /// The card's (or the dialog's) content for the current stage. `centered`: the dialog's layout.
    fn card_body(&mut self, ui: &mut Ui, centered: bool) {
        let k = theme::t(ui);
        let title = |ui: &mut Ui, t: &str| {
            ui.label(egui::RichText::new(t).font(theme::font_semibold(15.0)).color(k.text));
        };
        let sub = |ui: &mut Ui, t: &str| {
            ui.label(egui::RichText::new(t).font(theme::font(13.0)).color(k.text_2));
        };
        let body = |ui: &mut Ui, t: &str| {
            ui.add(egui::Label::new(egui::RichText::new(t).font(theme::font(13.0)).color(k.text_2)).wrap());
        };
        let mut action: Option<&'static str> = None;
        let stage = std::mem::replace(&mut self.updater.stage, Stage::Idle);
        // header: icon, title and line under it, × on the right (the card)
        let header = |ui: &mut Ui, icon: Icon, t: &str, s: &str, close: bool, action: &mut Option<&'static str>| {
            if centered {
                ui.vertical_centered(|ui| {
                    icon.draw(ui, 56.0);
                    ui.add_space(8.0);
                    title(ui, t);
                    if !s.is_empty() {
                        sub(ui, s);
                    }
                });
                return;
            }
            ui.horizontal_top(|ui| {
                icon.draw(ui, 40.0);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    title(ui, t);
                    if !s.is_empty() {
                        sub(ui, s);
                    }
                });
                if close {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("×").font(theme::font(16.0)).color(k.text_3)).frame(false)).on_hover_text("Close").clicked() {
                            *action = Some("close");
                        }
                    });
                }
            });
        };
        let indent = if centered { 0.0 } else { 56.0 };
        match &stage {
            Stage::Idle => {}
            Stage::Checking => {
                header(ui, Icon::App, "Checking for updates…", "", false, &mut action);
                ui.add_space(6.0);
                ui.vertical_centered(|ui| ui.spinner());
                ui.add_space(6.0);
                kit::footer(ui, |ui| {
                    if kit::btn(ui, "Cancel").clicked() {
                        action = Some("dismiss");
                    }
                });
            }
            Stage::Available(rel) => {
                let you = if centered { format!("You have {}. What's new:", update::VERSION) } else { format!("You have {}.", update::VERSION) };
                header(ui, Icon::App, &format!("Fermata {} is available", rel.version), &you, true, &mut action);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for n in &rel.notes {
                            ui.horizontal_top(|ui| {
                                ui.label(egui::RichText::new("•").font(theme::font(13.0)).color(k.text_2));
                                ui.add(egui::Label::new(egui::RichText::new(n).font(theme::font(13.0)).color(k.text)).wrap());
                            });
                        }
                        if (!centered || rel.notes.is_empty())
                            && ui.add(egui::Label::new(egui::RichText::new("Release notes").font(theme::font(13.0)).color(k.accent)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                        {
                            open_url(&rel.page);
                        }
                    });
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if !centered {
                        ui.add_space(indent);
                    }
                    self.checkbox(ui);
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::btn(ui, "Skip This Version").clicked() {
                        action = Some("skip");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if kit::btn_primary(ui, "Update").clicked() {
                            action = Some("update");
                        }
                        if kit::btn(ui, "Later").clicked() {
                            action = Some("close");
                        }
                    });
                });
            }
            Stage::Downloading(rel, job, demo) => {
                header(ui, Icon::App, &format!("Updating to Fermata {}", rel.version), "", true, &mut action);
                let (done, total) = job.as_ref().map(|j| j.progress()).unwrap_or((demo.0, demo.1));
                let left = job.as_ref().and_then(|j| j.secs_left()).or(if demo.2 > 0.0 { Some(demo.2) } else { None });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.vertical(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                        ui.painter().rect_filled(r, 3.0, k.cell);
                        let f = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * f, r.height())), 3.0, k.accent);
                        ui.horizontal(|ui| {
                            let t = if total > 0 { format!("Downloading... {} of {} MB", mb(done), mb(total)) } else { "Downloading...".into() };
                            ui.label(egui::RichText::new(t).font(theme::font(12.0)).color(k.text_3));
                            if let Some(s) = left {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(left_text(s)).font(theme::font(12.0)).color(k.text_3));
                                });
                            }
                        });
                        body(ui, "You can keep working. Fermata will ask before restarting.");
                    });
                });
                ui.add_space(4.0);
                kit::footer(ui, |ui| {
                    if kit::btn(ui, "Cancel").clicked() {
                        action = Some("cancel");
                    }
                });
            }
            Stage::Ready(rel, _) => {
                header(ui, Icon::Done, &format!("Fermata {} is ready.", rel.version), "Restart to finish.", true, &mut action);
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    body(ui, "Your score stays open. Fermata asks to save first if there are unsaved changes.");
                });
                ui.add_space(4.0);
                kit::footer(ui, |ui| {
                    if kit::btn_primary(ui, "Restart Now").clicked() {
                        action = Some("restart");
                    }
                    if kit::btn(ui, "Later").clicked() {
                        action = Some("close");
                    }
                });
            }
            Stage::Failed(_, e) => {
                let (text, retry) = match e {
                    UpdateError::Download(_) => ("The download didn't finish. Check your connection and try again.".to_string(), true),
                    UpdateError::Disk => ("There isn't enough space on the disk. Free some space and try again.".to_string(), true),
                    UpdateError::Security(_) => ("The update didn't pass the security check, so it wasn't installed.".to_string(), false),
                    UpdateError::Folder(_) if cfg!(target_os = "windows") => {
                        ("Fermata can't replace itself from here. Move the Fermata folder somewhere you can write, like Documents.".to_string(), false)
                    }
                    UpdateError::Folder(_) => ("Fermata can't replace itself from here. Move Fermata to Applications, then try again.".to_string(), false),
                    UpdateError::Cancelled => ("The update was cancelled.".to_string(), true),
                };
                header(ui, Icon::Warn, "Couldn't update Fermata", &format!("You still have {}.", update::VERSION), true, &mut action);
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    body(ui, &text);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if kit::btn(ui, "Open Download Page").clicked() {
                        action = Some("page");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if retry {
                            if kit::btn(ui, "Try Again").clicked() {
                                action = Some("update");
                            }
                        } else if kit::btn(ui, "OK").clicked() {
                            action = Some("close");
                        }
                    });
                });
            }
            Stage::Latest => {
                let when = self.updater.checked_at.map(|t| {
                    let m = now_secs().saturating_sub(t) / 60;
                    if m == 0 { "Last checked just now.".to_string() } else { format!("Last checked {m} minute{} ago.", if m == 1 { "" } else { "s" }) }
                });
                header(ui, Icon::Done, &format!("Fermata {} is the latest version", update::VERSION), when.as_deref().unwrap_or(""), false, &mut action);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| self.checkbox(ui));
                ui.add_space(6.0);
                kit::footer(ui, |ui| {
                    if kit::btn_primary(ui, "OK").clicked() {
                        action = Some("dismiss");
                    }
                });
            }
            Stage::Offline => {
                header(ui, Icon::Offline, "Couldn't check for updates", "", false, &mut action);
                ui.vertical_centered(|ui| body(ui, "No internet connection. Fermata will try again the next time it opens."));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::btn(ui, "Try Again").clicked() {
                        action = Some("recheck");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::btn_primary(ui, "OK").clicked() {
                            action = Some("dismiss");
                        }
                    });
                });
            }
        }
        self.updater.stage = stage;
        match action {
            // × and Later: the card goes until the next start (a download goes on; the status bar shows it)
            Some("close") => {
                self.updater.card = false;
                self.updater.manual = false;
                if let Stage::Failed(..) = self.updater.stage {
                    self.updater.stage = Stage::Idle;
                }
            }
            Some("dismiss") => {
                self.updater.manual = false;
                if !matches!(self.updater.stage, Stage::Downloading(..) | Stage::Ready(..) | Stage::Available(_)) {
                    self.updater.stage = Stage::Idle;
                }
            }
            Some("skip") => {
                if let Stage::Available(rel) = &self.updater.stage {
                    self.prefs.skipped_version = Some(rel.version.clone());
                    crate::prefs::save(&self.prefs);
                }
                self.updater.stage = Stage::Idle;
                self.updater.card = false;
                self.updater.manual = false;
            }
            Some("update") => {
                let rel = match &self.updater.stage {
                    Stage::Available(r) | Stage::Failed(r, _) => Some(r.clone()),
                    _ => None,
                };
                if let Some(r) = rel {
                    self.begin_update(r);
                }
            }
            Some("cancel") => {
                if let Stage::Downloading(rel, job, _) = &self.updater.stage {
                    match job {
                        Some(j) => j.cancel(),
                        None => self.updater.stage = Stage::Available(rel.clone()),
                    }
                }
            }
            Some("restart") => self.replace_score(Then::UpdateRestart),
            Some("page") => open_url(update::download_page()),
            Some("recheck") => self.start_update_check(true),
            _ => {}
        }
    }

    fn updated_toast(&mut self, ctx: &egui::Context) {
        let k = theme::ctx_tokens(ctx);
        let pos = pos2(ctx.content_rect().right() - 16.0, 84.0);
        let mut hide = false;
        let r = egui::Area::new(egui::Id::unique("update-toast")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::new()
                .fill(k.panel)
                .stroke(Stroke::new(1.0, k.line))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .shadow(egui::Shadow { offset: [0, 4], blur: 18, spread: 0, color: Color32::from_black_alpha(35) })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        Icon::App.draw(ui, 26.0);
                        ui.label(egui::RichText::new(format!("Updated to {}", update::VERSION)).font(theme::font_semibold(14.0)).color(k.text));
                        ui.label(egui::RichText::new("·").font(theme::font(14.0)).color(k.text_3));
                        if ui.add(egui::Label::new(egui::RichText::new("What's new").font(theme::font(14.0)).color(k.accent)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open_url(&update::release_page(update::VERSION));
                            hide = true;
                        }
                    });
                });
        });
        // a click anywhere else (on the score) hides it
        let clicked_elsewhere = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().map(|p| !r.response.rect.contains(p)).unwrap_or(false));
        if hide || clicked_elsewhere {
            self.updater.toast_until = None;
        }
    }

    /// `--update-demo <state>`: a state of the card or the dialog with a sample release, for screenshots.
    pub fn update_demo(&mut self, state: &str) {
        let rel = Release {
            version: "0.3.12".into(),
            page: update::release_page("0.3.12"),
            notes: vec!["Updates install themselves after you say yes".into(), "Import from photo was removed".into(), "Check for Updates... in the Help menu".into()],
            asset: None,
            sig: None,
        };
        let u = &mut self.updater;
        u.started = true;
        u.card = true;
        u.manual = false;
        u.checked_at = Some(now_secs());
        u.stage = match state {
            "available" => Stage::Available(rel),
            "downloading" => Stage::Downloading(rel, None, (5_100_000, 11_000_000, 20.0)),
            "ready" => Stage::Ready(rel, None),
            // Restart Now with unsaved changes: the app's save question comes first
            "ready-unsaved" => {
                self.dirty = true;
                self.updater.stage = Stage::Ready(rel, None);
                return;
            }
            "error-download" => Stage::Failed(rel, UpdateError::Download("demo".into())),
            "error-security" => Stage::Failed(rel, UpdateError::Security("demo".into())),
            "error-folder" => Stage::Failed(rel, UpdateError::Folder("demo".into())),
            "dialog-available" => {
                u.manual = true;
                Stage::Available(rel)
            }
            "latest" => {
                u.manual = true;
                Stage::Latest
            }
            "offline" => {
                u.manual = true;
                Stage::Offline
            }
            "updated" => {
                u.card = false;
                u.toast_until = Some(1e9);
                Stage::Idle
            }
            _ => Stage::Idle,
        };
    }
}

#[derive(Clone, Copy)]
enum Icon {
    App,
    Done,
    Warn,
    Offline,
}

impl Icon {
    fn draw(self, ui: &mut Ui, size: f32) {
        let k = theme::t(ui);
        match self {
            Icon::App => kit::alert_icon(ui, false, size),
            Icon::Done => {
                let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                ui.painter().rect_filled(r, size * 0.22, theme::alpha(k.accent, 0.14));
                let c = r.center();
                let rad = size * 0.3;
                ui.painter().circle_stroke(c, rad, Stroke::new(size * 0.06, k.accent));
                let s = Stroke::new(size * 0.07, k.accent);
                ui.painter().line_segment([c + vec2(-rad * 0.45, 0.0), c + vec2(-rad * 0.1, rad * 0.38)], s);
                ui.painter().line_segment([c + vec2(-rad * 0.1, rad * 0.38), c + vec2(rad * 0.5, -rad * 0.35)], s);
            }
            Icon::Warn | Icon::Offline => {
                let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                ui.painter().rect_filled(r, size * 0.22, theme::alpha(k.warn, 0.18));
                let glyph = if matches!(self, Icon::Warn) { "⚠" } else { "⌁" };
                ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, theme::font_semibold(size * 0.5), k.warn);
            }
        }
    }
}
