use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use serde::{Deserialize, Serialize};

use crate::sim::{Config, RunRecord, Simulation};
use crate::util::*;

const STORAGE_KEY: &str = "double-ou-moitie";
const HISTORY_MAX: usize = 80;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tab {
    #[default]
    Live,
    Tree,
    Stats,
    Theory,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TreeMode {
    #[default]
    Binary,
    Lattice,
}

/// Réglages d'affichage, sauvegardés entre deux lancements.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOpts {
    pub show_traj: bool,
    pub show_band: bool,
    pub show_median: bool,
    pub show_mean: bool,
    pub show_theory: bool,
    pub tables: usize,
    pub tree_mode: TreeMode,
    pub tree_depth: usize,
    pub lattice_depth: usize,
    pub tree_labels: bool,
    pub include_current: bool,
    pub notation: Notation,
}

impl Default for ViewOpts {
    fn default() -> Self {
        Self {
            show_traj: true,
            show_band: true,
            show_median: true,
            show_mean: true,
            show_theory: true,
            tables: 48,
            tree_mode: TreeMode::Binary,
            tree_depth: 6,
            lattice_depth: 16,
            tree_labels: true,
            include_current: true,
            notation: Notation::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    cfg: Config,
    history: Vec<RunRecord>,
    next_id: u64,
    speed: f32,
    max_speed: bool,
    random_seed: bool,
    tab: Tab,
    view: ViewOpts,
}

pub struct App {
    pub cfg: Config,
    pub sim: Option<Simulation>,
    pub running: bool,
    pub archived: bool,
    pub history: Vec<RunRecord>,
    pub next_id: u64,
    pub speed: f32,
    pub max_speed: bool,
    pub random_seed: bool,
    pub tab: Tab,
    pub view: ViewOpts,
    pub confirm_clear: bool,
    pub theory_cache: Option<(Config, usize, Vec<[f64; 4]>)>,
    step_debt: f64,
    rps: f64,
    /// `--capture=fichier.png` : enregistre une capture de la fenêtre puis quitte (pour la doc).
    capture: Option<Capture>,
}

struct Capture {
    path: String,
    frames: u32,
    requested: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let saved: Saved = cc
            .storage
            .and_then(|s| eframe::get_value(s, STORAGE_KEY))
            .unwrap_or_else(|| Saved {
                speed: 20.0,
                random_seed: true,
                next_id: 1,
                ..Default::default()
            });
        set_notation(saved.view.notation);
        let mut app = Self {
            cfg: saved.cfg,
            sim: None,
            running: false,
            archived: false,
            history: saved.history,
            next_id: saved.next_id.max(1),
            speed: if saved.speed > 0.0 { saved.speed } else { 20.0 },
            max_speed: saved.max_speed,
            random_seed: saved.random_seed,
            tab: saved.tab,
            view: saved.view,
            confirm_clear: false,
            theory_cache: None,
            step_debt: 0.0,
            rps: 0.0,
            capture: None,
        };
        // options de démarrage : --auto (lance tout de suite) et --onglet=arbre|stats|theorie
        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--auto" => app.launch(),
                "--onglet=arbre" => app.tab = Tab::Tree,
                "--onglet=stats" => app.tab = Tab::Stats,
                "--onglet=theorie" => app.tab = Tab::Theory,
                "--onglet=direct" => app.tab = Tab::Live,
                "--max" => app.max_speed = true,
                a if a.starts_with("--capture=") => {
                    app.capture = Some(Capture { path: a["--capture=".len()..].to_string(), frames: 0, requested: false });
                }
                _ => {}
            }
        }
        app
    }

    pub fn launch(&mut self) {
        if let Some(sim) = &self.sim
            && !self.archived
            && sim.round > 0
        {
            self.archive();
        }
        if self.random_seed {
            self.cfg.seed = fastrand::u64(..) % 1_000_000;
        }
        self.sim = Some(Simulation::new(self.cfg.clone()));
        self.running = true;
        self.archived = false;
        self.step_debt = 1.0;
        self.rps = 0.0;
    }

    pub fn archive(&mut self) {
        let Some(sim) = &self.sim else { return };
        if self.archived {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let rec = sim.to_record(id, format!("Partie #{id}"), (id - 1) as usize);
        self.history.push(rec);
        if self.history.len() > HISTORY_MAX {
            self.history.remove(0);
        }
        self.archived = true;
    }

    fn advance(&mut self, ctx: &egui::Context) {
        if !self.running {
            return;
        }
        let Some(sim) = &mut self.sim else { return };
        let dt = (ctx.input(|i| i.stable_dt) as f64).clamp(1e-3, 0.1);
        let t0 = Instant::now();
        let budget = Duration::from_millis(15);
        let before = sim.round;
        if self.max_speed {
            while sim.step() && t0.elapsed() < budget {}
        } else {
            let speed = self.speed as f64;
            self.step_debt = (self.step_debt + speed * dt).min(speed * 0.2 + 1.0);
            while self.step_debt >= 1.0 && t0.elapsed() < budget {
                if !sim.step() {
                    break;
                }
                self.step_debt -= 1.0;
            }
        }
        let done = (sim.round - before) as f64;
        self.rps = 0.85 * self.rps + 0.15 * done / dt;
        if sim.finished() {
            self.running = false;
            self.archive();
        }
        ctx.request_repaint();
    }

    fn status(&self) -> (&'static str, Color32) {
        match &self.sim {
            None => ("Prêt", MUTED),
            Some(s) if s.alive == 0 => ("Tous ruinés", RED),
            Some(s) if s.stopped => ("Arrêtée", GOLD),
            Some(s) if s.finished() => ("Terminée", GREEN),
            Some(_) if self.running => ("En cours", BLUE),
            Some(_) => ("En pause", GOLD),
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("🎲 Double ou Moitié").size(24.0).strong().color(GOLD));
            ui.add_space(24.0);
            for (tab, label) in [
                (Tab::Live, "▶  Simulation en direct"),
                (Tab::Tree, "🌳  Arbre des probabilités"),
                (Tab::Stats, "📊  Statistiques & comparaison"),
                (Tab::Theory, "💡  Comprendre le jeu"),
            ] {
                let selected = self.tab == tab;
                let text = RichText::new(label).size(15.5);
                let text = if selected { text.strong().color(Color32::WHITE) } else { text };
                if ui.selectable_label(selected, text).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, color) = self.status();
                egui::Frame::new()
                    .fill(color.gamma_multiply(0.15))
                    .stroke(Stroke::new(1.0, color))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            dot(ui, color, 5.0);
                            ui.label(RichText::new(label).color(color).strong());
                        });
                    });
            });
        });
    }

    fn config_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        section(ui, "⚙  Règles du jeu");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Jeu de base").on_hover_text("Mise tout, ×2 ou ×0,5, 50 %").clicked() {
                let keep = (self.cfg.n_sims, self.cfg.n_rounds, self.cfg.n_tracked, self.cfg.seed);
                self.cfg = Config::default();
                (self.cfg.n_sims, self.cfg.n_rounds, self.cfg.n_tracked, self.cfg.seed) = keep;
            }
            if ui.button("Pièce favorable").on_hover_text("55 % de chances de gagner").clicked() {
                self.cfg.p_win = 0.55;
            }
            if ui.button("Mise de Kelly").on_hover_text("Ne miser que la part optimale du capital").clicked() {
                self.cfg.bet_frac = crate::sim::kelly(&self.cfg).0.max(0.01);
            }
            if ui.button("Sans ruine").on_hover_text("Seuil de ruine quasi nul").clicked() {
                self.cfg.ruin = 1e-12;
            }
        });
        ui.add_space(4.0);
        let c = &mut self.cfg;
        egui::Grid::new("cfg").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("Capital de départ");
            ui.add(egui::DragValue::new(&mut c.start).range(0.01..=1_000_000.0).speed(0.05).suffix(" €"));
            ui.end_row();

            ui.label("Chance de gagner");
            let mut pct = c.p_win * 100.0;
            ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %").fixed_decimals(1));
            c.p_win = pct / 100.0;
            ui.end_row();

            ui.label("Si on gagne, mise ×");
            ui.add(egui::DragValue::new(&mut c.win_mult).range(1.0..=10.0).speed(0.01).max_decimals(3));
            ui.end_row();

            ui.label("Si on perd, mise ×");
            ui.add(egui::DragValue::new(&mut c.loss_mult).range(0.01..=1.0).speed(0.005).max_decimals(3));
            ui.end_row();

            ui.label("Part du capital misée");
            let mut f = c.bet_frac * 100.0;
            ui.add(egui::Slider::new(&mut f, 1.0..=100.0).suffix(" %").fixed_decimals(0));
            c.bet_frac = f / 100.0;
            ui.end_row();

            ui.label("Ruiné à partir de");
            ui.add(
                egui::DragValue::new(&mut c.ruin)
                    .range(0.0..=c.start)
                    .speed(0.001)
                    .max_decimals(4)
                    .suffix(" €"),
            )
            .on_hover_text("Un joueur dont le capital tombe à ce montant ou moins ne peut plus jouer");
            ui.end_row();
        });
        if self.cfg.ruin <= 0.0 {
            self.cfg.ruin = 1e-12;
        }

        ui.add_space(10.0);
        section(ui, "🔬  Simulation");
        let c = &mut self.cfg;
        egui::Grid::new("sim").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            ui.label("Joueurs simulés");
            ui.add(egui::Slider::new(&mut c.n_sims, 1..=2_000_000).logarithmic(true));
            ui.end_row();

            ui.label("Tours par joueur");
            ui.add(egui::Slider::new(&mut c.n_rounds, 1..=5_000).logarithmic(true));
            ui.end_row();

            ui.label("Courbes affichées");
            ui.add(egui::Slider::new(&mut c.n_tracked, 0..=300))
                .on_hover_text("Nombre de joueurs dont on trace la trajectoire individuelle");
            ui.end_row();

            ui.label("Graine aléatoire");
            ui.horizontal(|ui| {
                ui.add_enabled(!self.random_seed, egui::DragValue::new(&mut c.seed));
                ui.checkbox(&mut self.random_seed, "nouvelle à chaque fois");
            });
            ui.end_row();
        });

        ui.add_space(10.0);
        section(ui, "🎮  Contrôle");
        let can_resume = self.sim.as_ref().is_some_and(|s| !s.finished());
        ui.horizontal_wrapped(|ui| {
            let big = |t: &str| RichText::new(t).size(16.0).strong();
            if ui
                .add(egui::Button::new(big("▶  Lancer")).fill(Color32::from_rgb(37, 99, 235)).min_size(Vec2::new(120.0, 34.0)))
                .on_hover_text("Nouvelle simulation avec ces réglages")
                .clicked()
            {
                self.launch();
            }
            if can_resume {
                if self.running {
                    if ui.add(egui::Button::new(big("⏸  Pause")).min_size(Vec2::new(100.0, 34.0))).clicked() {
                        self.running = false;
                    }
                } else if ui.add(egui::Button::new(big("⏵  Reprendre")).min_size(Vec2::new(100.0, 34.0))).clicked() {
                    self.running = true;
                }
            }
        });
        if can_resume {
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.running, egui::Button::new("⏭  +1 tour")).clicked()
                    && let Some(s) = &mut self.sim
                {
                    s.step();
                    if s.finished() {
                        self.archive();
                    }
                }
                if ui.button("⏹  Terminer et archiver").clicked() {
                    self.running = false;
                    self.archive();
                    if let Some(s) = &mut self.sim {
                        s.stopped = true;
                    }
                }
            });
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Vitesse");
            ui.add_enabled(
                !self.max_speed,
                egui::Slider::new(&mut self.speed, 1.0..=500.0).logarithmic(true).suffix(" tours/s").fixed_decimals(0),
            );
        });
        ui.checkbox(&mut self.max_speed, "Vitesse maximale (tous les cœurs du processeur)");

        if let Some(sim) = &self.sim {
            ui.add_space(6.0);
            let text = format!("Tour {} / {}", sim.round, sim.cfg.n_rounds);
            ui.add(egui::ProgressBar::new(sim.progress()).text(text).desired_height(22.0));
            ui.label(
                RichText::new(format!(
                    "{} joueurs · {} tours/s · calcul {:.2} s · graine {}",
                    fmt_int(sim.n() as u64),
                    fmt_int(if self.running { self.rps.round() as u64 } else { 0 }),
                    sim.compute_secs,
                    sim.cfg.seed
                ))
                .small()
                .color(MUTED),
            );
            if sim.cfg != self.cfg {
                ui.label(RichText::new("Réglages modifiés : ils s'appliqueront au prochain lancement.").small().color(GOLD));
            }
        }

        ui.add_space(10.0);
        section(ui, "🔢  Affichage des montants");
        for n in [Notation::Suffixes, Notation::Mots, Notation::Scientifique] {
            if ui.radio_value(&mut self.view.notation, n, n.label()).changed() {
                set_notation(n);
            }
        }
        if self.view.notation == Notation::Suffixes {
            ui.label(
                RichText::new(
                    "K = mille · M = million · B = milliard · T = 1 000 milliards · \
                     puis Qa, Qi, Sx, Sp, Oc, No, Dc (×1 000 à chaque fois), puis aa, ab, ac…",
                )
                .small()
                .color(MUTED),
            );
        }
        ui.label(RichText::new("Survole un montant pour voir les autres écritures.").small().color(MUTED));

        ui.add_space(10.0);
        section(ui, "📌  Repères");
        let c = &self.cfg;
        let e = c.expected_factor();
        let g = c.geometric_factor();
        ui.label(format!("Espérance par tour : ×{}", fmt_num(e)));
        ui.label(format!("Joueur typique par tour : ×{}", fmt_num(g)));
        let pertes = ((c.ruin / c.start).ln() / c.down().ln()).ceil();
        if pertes.is_finite() && pertes > 0.0 {
            ui.label(format!("Ruiné après {} pertes nettes d'affilée", pertes as i64));
        }
        let lose = c.lc(1, 0).exp();
        let win = c.lc(1, 1).exp();
        ui.label(
            RichText::new(format!(
                "1er tour : {} devient {} ou {}",
                fmt_eur(c.start),
                fmt_eur(win),
                fmt_eur(lose)
            ))
            .color(MUTED),
        );
    }
}

impl App {
    fn handle_capture(&mut self, ctx: &egui::Context) {
        let Some(cap) = &mut self.capture else { return };
        ctx.request_repaint();
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                && let Err(e) = buf.save(&cap.path)
            {
                eprintln!("capture impossible : {e}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if !cap.requested {
            cap.frames += 1;
            let ready = self.sim.as_ref().is_none_or(|s| s.finished());
            if ready && cap.frames > 40 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                cap.requested = true;
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.advance(&ctx);
        self.handle_capture(&ctx);

        egui::Panel::top("top")
            .frame(egui::Frame::new().fill(Color32::from_rgb(13, 17, 27)).inner_margin(egui::Margin::symmetric(16, 10)))
            .show_inside(ui, |ui| self.top_bar(ui));

        egui::Panel::left("config")
            .resizable(true)
            .default_size(340.0)
            .min_size(300.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(20, 25, 38)).inner_margin(14))
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| self.config_panel(ui));
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(15, 19, 30)).inner_margin(14))
            .show_inside(ui, |ui| match self.tab {
                Tab::Live => self.live_tab(ui),
                Tab::Tree => self.tree_tab(ui),
                Tab::Stats => self.stats_tab(ui),
                Tab::Theory => self.theory_tab(ui),
            });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let saved = Saved {
            cfg: self.cfg.clone(),
            history: self.history.clone(),
            next_id: self.next_id,
            speed: self.speed,
            max_speed: self.max_speed,
            random_seed: self.random_seed,
            tab: self.tab,
            view: self.view.clone(),
        };
        eframe::set_value(storage, STORAGE_KEY, &saved);
    }
}

/// Pastille de couleur dessinée (la police n'a pas le caractère ●).
pub fn dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 2.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(16.0).strong().color(Color32::from_rgb(226, 232, 240)));
    ui.add_space(2.0);
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .corner_radius(10)
        .inner_margin(12)
        .show(ui, add)
        .inner
}

/// Petite tuile « indicateur » : titre, valeur, ligne secondaire.
pub fn kpi(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    value: &str,
    color: Color32,
    sub: Option<(&str, Color32)>,
) -> egui::Response {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).small().color(MUTED));
                ui.label(RichText::new(value).size(20.0).strong().color(color));
                if let Some((s, c)) = sub {
                    ui.label(RichText::new(s).small().color(c));
                }
            });
        })
        .response
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(17, 21, 32);
    v.window_fill = Color32::from_rgb(24, 29, 43);
    v.extreme_bg_color = Color32::from_rgb(11, 14, 22);
    v.faint_bg_color = Color32::from_rgb(26, 31, 46);
    v.selection.bg_fill = Color32::from_rgb(37, 99, 235);
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = BLUE;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 6.into();
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 43, 62);
    v.widgets.inactive.bg_fill = Color32::from_rgb(36, 43, 62);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 58, 84);
    v.window_corner_radius = 10.into();
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.slider_width = 150.0;
    });
}
