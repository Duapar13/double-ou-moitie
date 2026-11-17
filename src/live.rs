use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use egui_plot::{Bar, BarChart, Corner, FilledArea, HLine, Legend, Line, LineStyle, Plot, PlotPoints};

use crate::app::{App, card, kpi};
use crate::util::*;

/// Nombre maximal de points tracés par courbe (au-delà on sous-échantillonne).
const MAX_POINTS: usize = 800;
/// Largeur d'une tuile « table de jeu », marges comprises.
const TILE_W: f32 = 118.0;

impl App {
    pub fn live_tab(&mut self, ui: &mut egui::Ui) {
        if self.sim.is_none() {
            if welcome(ui) {
                self.launch();
            }
            return;
        }
        let App { sim, view, .. } = self;
        let sim = sim.as_ref().expect("vérifié plus haut");
        let last = *sim.last();
        let start_l10 = sim.cfg.start.log10();
        let ruin_l10 = sim.cfg.ruin.log10();
        let n = sim.n() as f64;

        // ── indicateurs ─────────────────────────────────────────────
        let gap = ui.spacing().item_spacing.x;
        let w = ((ui.available_width() - 6.0 * gap) / 7.0 - 26.0).max(90.0);
        ui.horizontal(|ui| {
            kpi(
                ui,
                w,
                "Tour",
                &format!("{} / {}", sim.round, sim.cfg.n_rounds),
                Color32::WHITE,
                Some((&format!("{} encore en jeu", fmt_int(sim.alive)), MUTED)),
            );
            kpi(
                ui,
                w,
                "Capital moyen",
                &fmt_eur_l10(last.mean),
                VIOLET,
                Some((&format!("théorie : {}", fmt_eur_l10(last.theo_mean)), MUTED)),
            )
            .on_hover_text(format!("Capital moyen :\n{}\n\nThéorie :\n{}", eur_detail(last.mean), eur_detail(last.theo_mean)));
            kpi(
                ui,
                w,
                "Capital médian",
                &fmt_eur_l10(last.median),
                GOLD,
                Some((&format!("théorie : {}", fmt_eur_l10(last.theo_median)), MUTED)),
            )
            .on_hover_text(format!("Capital médian :\n{}", eur_detail(last.median)));
            kpi(
                ui,
                w,
                "Ruinés",
                &fmt_pct(last.ruined),
                RED,
                Some((&format!("théorie : {}", fmt_pct(last.theo_ruined)), MUTED)),
            );
            kpi(
                ui,
                w,
                "Gagnants (> départ)",
                &fmt_pct(last.winners),
                GREEN,
                Some((&format!("théorie : {}", fmt_pct(last.theo_winners)), MUTED)),
            );
            kpi(
                ui,
                w,
                "Meilleur joueur",
                &fmt_eur_l10(last.max),
                GOLD,
                Some((&format!("pire : {}", fmt_eur_l10(last.min)), MUTED)),
            )
            .on_hover_text(format!("Meilleur joueur :\n{}", eur_detail(last.max)));
            let total = last.mean + n.log10();
            let start_total = start_l10 + n.log10();
            let up = total > start_total;
            kpi(
                ui,
                w,
                "Argent total en jeu",
                &fmt_eur_l10(total),
                if up { GREEN } else { RED },
                Some((&format!("au départ : {}", fmt_eur_l10(start_total)), MUTED)),
            )
            .on_hover_text(format!("Argent total en jeu :\n{}", eur_detail(total)));
        });
        ui.add_space(8.0);

        // ── trajectoires ────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(RichText::new("Évolution du capital (échelle logarithmique)").size(16.0).strong());
            ui.add_space(16.0);
            ui.checkbox(&mut view.show_traj, "Joueurs individuels");
            ui.checkbox(&mut view.show_band, "Zones 10–90 % et 25–75 %");
            ui.checkbox(&mut view.show_median, "Médiane");
            ui.checkbox(&mut view.show_mean, "Moyenne");
            ui.checkbox(&mut view.show_theory, "Théorie");
        });
        let plot_h = ((ui.available_height() - 8.0) * 0.56).max(240.0);
        let hist = &sim.history;
        let stride = (hist.len() / MAX_POINTS).max(1);
        let sampled: Vec<_> = sampled(hist, stride);
        let xs: Vec<f64> = sampled.iter().map(|s| s.round as f64).collect();
        let series = |f: fn(&crate::sim::RoundStats) -> f64| -> PlotPoints<'static> {
            sampled.iter().map(|s| [s.round as f64, f(s)]).collect()
        };
        let states: Vec<(i8, f64, bool)> = sim.tracked_state().collect();

        card(ui, |ui| {
            Plot::new("trajectoires")
                .height(plot_h)
                .legend(Legend::default().position(Corner::LeftTop))
                .x_axis_label("tour")
                .y_axis_formatter(|m, _| axis_eur(m.value))
                .y_grid_spacer(decade_spacer)
                .y_axis_min_width(60.0)
                .include_x(0.0)
                .include_x(sim.cfg.n_rounds.min(sim.round.max(10)) as f64)
                .include_y(start_l10)
                .label_formatter(|name, p| {
                    let head = if name.is_empty() { String::new() } else { format!("{name}\n") };
                    format!("{head}tour {:.0}\n{}", p.x, fmt_eur_l10(p.y))
                })
                .show(ui, |pu| {
                    if view.show_band && xs.len() > 1 {
                        let p10: Vec<f64> = sampled.iter().map(|s| s.p10).collect();
                        let p90: Vec<f64> = sampled.iter().map(|s| s.p90).collect();
                        let p25: Vec<f64> = sampled.iter().map(|s| s.p25).collect();
                        let p75: Vec<f64> = sampled.iter().map(|s| s.p75).collect();
                        pu.add(
                            FilledArea::new("80 % des joueurs", &xs, &p10, &p90)
                                .fill_color(BLUE.gamma_multiply(0.10))
                                .stroke(Stroke::NONE),
                        );
                        pu.add(
                            FilledArea::new("50 % des joueurs", &xs, &p25, &p75)
                                .fill_color(BLUE.gamma_multiply(0.20))
                                .stroke(Stroke::NONE),
                        );
                    }
                    if view.show_traj {
                        let tstride = (sim.round / MAX_POINTS).max(1);
                        for (traj, (_, now, alive)) in sim.tracked.iter().zip(&states) {
                            let mut pts: Vec<[f64; 2]> =
                                traj.iter().enumerate().step_by(tstride).map(|(i, v)| [i as f64, *v as f64]).collect();
                            if let Some(v) = traj.last() {
                                pts.push([(traj.len() - 1) as f64, *v as f64]);
                            }
                            let c = money_color(*now, start_l10, !alive);
                            pu.line(Line::new("", pts).color(c.gamma_multiply(0.35)).width(1.0));
                        }
                    }
                    pu.hline(
                        HLine::new("Mise de départ", start_l10)
                            .color(MUTED.gamma_multiply(0.7))
                            .style(LineStyle::dashed_loose()),
                    );
                    if ruin_l10 > -10.0 {
                        pu.hline(
                            HLine::new("Seuil de ruine", ruin_l10)
                                .color(RED.gamma_multiply(0.8))
                                .style(LineStyle::dashed_loose()),
                        );
                    }
                    if view.show_theory {
                        if view.show_mean {
                            pu.line(
                                Line::new("Moyenne théorique", series(|s| s.theo_mean))
                                    .color(VIOLET.gamma_multiply(0.8))
                                    .style(LineStyle::dashed_dense())
                                    .width(1.5),
                            );
                        }
                        if view.show_median {
                            pu.line(
                                Line::new("Médiane théorique", series(|s| s.theo_median))
                                    .color(GOLD.gamma_multiply(0.8))
                                    .style(LineStyle::dashed_dense())
                                    .width(1.5),
                            );
                        }
                    }
                    if view.show_mean {
                        pu.line(Line::new("Moyenne", series(|s| s.mean)).color(VIOLET).width(2.5));
                    }
                    if view.show_median {
                        pu.line(Line::new("Médiane", series(|s| s.median)).color(GOLD).width(2.5));
                    }
                });
        });
        ui.add_space(8.0);

        // ── répartition + tables ────────────────────────────────────
        let bottom_h = ui.available_height() - 30.0;
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                ui.label(RichText::new(format!("Répartition des capitaux au tour {}", sim.round)).strong());
                histogram(ui, &sim.hist, n, start_l10, ruin_l10, bottom_h - 30.0);
            });
            card(&mut cols[1], |ui| {
                let k = sim.tracked.len();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tables de jeu").strong());
                    if k > 0 {
                        ui.add(egui::Slider::new(&mut view.tables, 1..=k.max(1)).text("joueurs"));
                    }
                });
                if k == 0 {
                    ui.label(RichText::new("Augmente « Courbes affichées » à gauche pour voir des joueurs.").color(MUTED));
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(bottom_h - 40.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let per_row = ((ui.available_width() + 6.0) / (TILE_W + 6.0)).floor().max(1.0) as usize;
                        egui::Grid::new("tables").spacing([6.0, 6.0]).show(ui, |ui| {
                            for (i, (last, l10, alive)) in states.iter().take(view.tables).enumerate() {
                                table_tile(ui, i + 1, *last, *l10, *alive, start_l10);
                                if (i + 1) % per_row == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
            });
        });
    }
}

fn sampled(hist: &[crate::sim::RoundStats], stride: usize) -> Vec<crate::sim::RoundStats> {
    let mut v: Vec<_> = hist.iter().step_by(stride).copied().collect();
    if let (Some(a), Some(b)) = (v.last(), hist.last())
        && a.round != b.round
    {
        v.push(*b);
    }
    v
}

pub fn histogram(ui: &mut egui::Ui, hist: &[(f64, u64)], n: f64, start_l10: f64, ruin_l10: f64, height: f32) {
    let mut gap = f64::INFINITY;
    for w in hist.windows(2) {
        gap = gap.min(w[1].0 - w[0].0);
    }
    let width = if gap.is_finite() { (gap * 0.8).min(0.5) } else { 0.25 };
    let bars: Vec<Bar> = hist
        .iter()
        .map(|&(x, k)| {
            let ruined = x <= ruin_l10 + 1e-9;
            Bar::new(x, k as f64 / n * 100.0).width(width).fill(money_color(x, start_l10, ruined))
        })
        .collect();
    let chart = BarChart::new("Joueurs", bars).element_formatter(Box::new(|b, _| {
        format!("{}\n{} des joueurs", fmt_eur_l10(b.argument), fmt_pct(b.value / 100.0))
    }));
    Plot::new("histo")
        .height(height.max(150.0))
        .x_axis_formatter(|m, _| axis_eur(m.value))
        .x_grid_spacer(decade_spacer)
        .y_axis_formatter(|m, _| format!("{:.0} %", m.value))
        .include_y(0.0)
        .allow_scroll(false)
        .show(ui, |pu| pu.bar_chart(chart));
}

fn table_tile(ui: &mut egui::Ui, i: usize, last: i8, l10: f64, alive: bool, start_l10: f64) {
    let (fill, arrow, acol) = if !alive && last == 0 {
        (DARK_RED.gamma_multiply(0.35), "ruiné", RED)
    } else if !alive {
        (DARK_RED.gamma_multiply(0.6), "ruiné !", RED)
    } else if last > 0 {
        (GREEN.gamma_multiply(0.16), "gagné", GREEN)
    } else if last < 0 {
        (RED.gamma_multiply(0.16), "perdu", RED)
    } else {
        (CARD, "", MUTED)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, CARD_STROKE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(TILE_W - 18.0);
            ui.set_min_height(40.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Joueur {i}")).small().color(MUTED));
                    ui.label(RichText::new(arrow).small().color(acol));
                });
                let color = if alive { money_color(l10, start_l10, false) } else { RED };
                ui.label(RichText::new(fmt_eur_l10(l10)).strong().color(color)).on_hover_text(eur_detail(l10));
            });
        });
}

/// Écran d'accueil. Renvoie true si on clique sur « Lancer ».
fn welcome(ui: &mut egui::Ui) -> bool {
    let mut go = false;
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.12);
        ui.set_max_width(760.0);
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("🎲").size(56.0));
                ui.label(RichText::new("Double ou Moitié").size(32.0).strong().color(GOLD));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Tu mises 1 €. Une chance sur deux de repartir avec 2 €, une chance sur deux avec 0,50 €.\n\
                         Tu remises tout à chaque tour : à 2 € tu peux finir à 4 € ou 1 €, et ainsi de suite.\n\
                         Quand il ne te reste qu'un centime, tu ne peux plus jouer.",
                    )
                    .size(16.0),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "En moyenne le jeu rapporte +25 % par tour… alors pourquoi presque tout le monde finit ruiné ?",
                    )
                    .size(15.0)
                    .italics()
                    .color(MUTED),
                );
                ui.add_space(18.0);
                go = ui
                    .add(
                        egui::Button::new(RichText::new("▶  Lancer la simulation").size(20.0).strong())
                            .fill(Color32::from_rgb(37, 99, 235))
                            .min_size(Vec2::new(280.0, 48.0)),
                    )
                    .clicked();
                ui.add_space(8.0);
                ui.label(RichText::new("Règle les paramètres dans le panneau de gauche.").small().color(MUTED));
            });
        });
    });
    go
}
