use eframe::egui::{self, RichText, Sense, Stroke, Vec2};
use egui_plot::{Corner, Legend, Line, LineStyle, Plot, PlotPoints, Points};

use crate::app::{App, card};
use crate::sim::{RoundStats, RunRecord};
use crate::util::*;

#[derive(Clone, Copy)]
enum Scale {
    Money,
    Percent,
}

impl App {
    pub fn stats_tab(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("runs")
            .resizable(true)
            .default_size(360.0)
            .min_size(280.0)
            .frame(egui::Frame::new().inner_margin(egui::Margin { right: 12, ..Default::default() }))
            .show_inside(ui, |ui| self.runs_list(ui));

        let mut runs: Vec<RunRecord> = self.history.iter().filter(|r| r.selected).cloned().collect();
        if self.view.include_current
            && !self.archived
            && let Some(sim) = &self.sim
            && sim.round > 0
        {
            let mut r = sim.to_record(0, "Partie en cours".into(), self.next_id.saturating_sub(1) as usize);
            r.color = 9;
            runs.push(r);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show_inside(ui, |ui| comparison(ui, &runs));
    }

    fn runs_list(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(format!("Parties archivées ({})", self.history.len())).size(17.0).strong());
        ui.label(
            RichText::new("Chaque simulation terminée est gardée ici, même après avoir fermé l'application.")
                .small()
                .color(MUTED),
        );
        ui.add_space(4.0);
        if self.sim.is_some() && !self.archived {
            ui.checkbox(&mut self.view.include_current, "Inclure la partie en cours");
        }
        ui.horizontal_wrapped(|ui| {
            let h = &mut self.history;
            if ui.small_button("Toutes").clicked() {
                h.iter_mut().for_each(|r| r.selected = true);
            }
            if ui.small_button("Aucune").clicked() {
                h.iter_mut().for_each(|r| r.selected = false);
            }
            if ui.small_button("Dernière").clicked() {
                let len = h.len();
                h.iter_mut().enumerate().for_each(|(i, r)| r.selected = i + 1 == len);
            }
            if ui.small_button("3 dernières").clicked() {
                let len = h.len();
                h.iter_mut().enumerate().for_each(|(i, r)| r.selected = i + 3 >= len);
            }
            if ui
                .small_button("Mêmes règles que la dernière")
                .on_hover_text("Sélectionne les parties jouées avec les mêmes règles (hors nombre de joueurs, tours et graine)")
                .clicked()
                && let Some(last) = h.last().map(|r| r.config.clone())
            {
                for r in h.iter_mut() {
                    let c = &r.config;
                    r.selected = c.p_win == last.p_win
                        && c.win_mult == last.win_mult
                        && c.loss_mult == last.loss_mult
                        && c.bet_frac == last.bet_frac
                        && c.ruin == last.ruin
                        && c.start == last.start;
                }
            }
        });
        ui.add_space(4.0);

        let mut delete = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 40.0).show(ui, |ui| {
            if self.history.is_empty() {
                ui.label(RichText::new("Aucune partie pour l'instant. Lance une simulation !").color(MUTED));
            }
            for r in self.history.iter_mut().rev() {
                let color = palette(r.color);
                let stroke = if r.selected { Stroke::new(1.5, color) } else { Stroke::new(1.0, CARD_STROKE) };
                egui::Frame::new()
                    .fill(if r.selected { CARD } else { CARD.gamma_multiply(0.6) })
                    .stroke(stroke)
                    .corner_radius(8)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut r.selected, "");
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                            ui.painter().circle_filled(rect.center(), 6.0, color);
                            ui.add(egui::TextEdit::singleline(&mut r.name).desired_width(150.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("🗑").on_hover_text("Supprimer cette partie").clicked() {
                                    delete = Some(r.id);
                                }
                                ui.label(RichText::new(ago(r.created)).small().color(MUTED));
                            });
                        });
                        ui.label(RichText::new(r.config.short_label()).small().color(MUTED));
                        let f = &r.final_stats;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("moy. {}", fmt_eur_l10(f.mean))).small().color(VIOLET));
                            ui.label(RichText::new(format!("méd. {}", fmt_eur_l10(f.median))).small().color(GOLD));
                            ui.label(RichText::new(format!("ruinés {}", fmt_pct(f.ruined))).small().color(RED));
                            if r.rounds_played < r.config.n_rounds && f.ruined < 1.0 {
                                ui.label(RichText::new(format!("arrêtée au tour {}", r.rounds_played)).small().color(GOLD));
                            }
                        });
                    });
                ui.add_space(4.0);
            }
        });
        if let Some(id) = delete {
            self.history.retain(|r| r.id != id);
        }
        ui.horizontal(|ui| {
            if self.confirm_clear {
                ui.label(RichText::new("Tout effacer ?").color(RED));
                if ui.button("Oui, effacer").clicked() {
                    self.history.clear();
                    self.confirm_clear = false;
                }
                if ui.button("Annuler").clicked() {
                    self.confirm_clear = false;
                }
            } else if ui.add_enabled(!self.history.is_empty(), egui::Button::new("🗑  Vider l'historique")).clicked() {
                self.confirm_clear = true;
            }
        });
    }
}

fn comparison(ui: &mut egui::Ui, runs: &[RunRecord]) {
    if runs.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.label(RichText::new("📊").size(48.0));
            ui.label(RichText::new("Sélectionne une ou plusieurs parties à gauche pour les comparer.").size(17.0));
            ui.label(RichText::new("Les simulations terminées y apparaissent automatiquement.").color(MUTED));
        });
        return;
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.label(RichText::new("Comparaison").size(17.0).strong());
            ui.add_space(4.0);
            egui::ScrollArea::horizontal().id_salt("table").show(ui, |ui| table(ui, runs));
        });
        ui.add_space(10.0);

        let w = ui.available_width();
        let h = 270.0;
        ui.columns(2, |c| {
            chart(&mut c[0], "median", "Capital médian (le joueur « typique »)", runs, h, Scale::Money, |s| s.median, Some(|s| s.theo_median));
            chart(&mut c[1], "mean", "Capital moyen (pointillés : espérance théorique)", runs, h, Scale::Money, |s| s.mean, Some(|s| s.theo_mean));
        });
        ui.add_space(10.0);
        ui.columns(2, |c| {
            chart(&mut c[0], "ruined", "Joueurs ruinés", runs, h, Scale::Percent, |s| s.ruined, Some(|s| s.theo_ruined));
            chart(&mut c[1], "winners", "Joueurs au-dessus de leur mise de départ", runs, h, Scale::Percent, |s| s.winners, Some(|s| s.theo_winners));
        });
        ui.add_space(10.0);
        ui.columns(2, |c| {
            chart(&mut c[0], "best", "Meilleur joueur", runs, h, Scale::Money, |s| s.max, None);
            chart(&mut c[1], "p90", "Seuil des 10 % les plus chanceux", runs, h, Scale::Money, |s| s.p90, None);
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.set_width(w - 26.0);
            ui.label(RichText::new("Répartition finale des capitaux").strong());
            Plot::new("dist-final")
                .height(300.0)
                .legend(Legend::default().position(Corner::RightTop))
                .x_axis_formatter(|m, _| axis_eur(m.value))
                .x_grid_spacer(decade_spacer)
                .y_axis_formatter(|m, _| format!("{:.0} %", m.value))
                .include_y(0.0)
                .allow_scroll(false)
                .label_formatter(|name, p| format!("{name}\n{}\n{} des joueurs", fmt_eur_l10(p.x), fmt_pct(p.y / 100.0)))
                .show(ui, |pu| {
                    for r in runs {
                        let n = r.config.n_sims as f64;
                        let pts: Vec<[f64; 2]> = r.final_hist.iter().map(|&(x, k)| [x, k as f64 / n * 100.0]).collect();
                        let c = palette(r.color);
                        pu.line(Line::new(r.name.clone(), pts.clone()).color(c.gamma_multiply(0.7)).width(1.5));
                        pu.points(Points::new(r.name.clone(), pts).color(c).radius(3.5));
                    }
                });
        });
    });
}

fn table(ui: &mut egui::Ui, runs: &[RunRecord]) {
    egui::Grid::new("cmp").striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
        for h in [
            "",
            "Partie",
            "Règles",
            "Joueurs",
            "Tours joués",
            "Capital moyen",
            "Espérance théo.",
            "Médiane",
            "Ruinés",
            "Gagnants",
            "Meilleur",
            "Calcul",
        ] {
            ui.label(RichText::new(h).small().strong().color(MUTED));
        }
        ui.end_row();
        for r in runs {
            let f = &r.final_stats;
            let c = &r.config;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 6.0, palette(r.color));
            ui.label(RichText::new(&r.name).strong());
            let mut rules = format!("p={} · ×{}/×{}", fmt_pct(c.p_win), fmt_num(c.win_mult), fmt_num(c.loss_mult));
            if (c.bet_frac - 1.0).abs() > 1e-9 {
                rules += &format!(" · mise {}", fmt_pct(c.bet_frac));
            }
            ui.label(rules);
            ui.label(fmt_int(c.n_sims as u64));
            ui.label(format!("{} / {}", r.rounds_played, c.n_rounds));
            ui.label(RichText::new(fmt_eur_l10(f.mean)).color(VIOLET));
            ui.label(RichText::new(fmt_eur_l10(f.theo_mean)).color(MUTED));
            ui.label(RichText::new(fmt_eur_l10(f.median)).color(GOLD));
            ui.label(RichText::new(fmt_pct(f.ruined)).color(RED));
            ui.label(RichText::new(fmt_pct(f.winners)).color(GREEN));
            ui.label(fmt_eur_l10(f.max));
            ui.label(RichText::new(format!("{:.2} s", r.compute_secs)).color(MUTED));
            ui.end_row();
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn chart(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    runs: &[RunRecord],
    height: f32,
    scale: Scale,
    f: fn(&RoundStats) -> f64,
    theo: Option<fn(&RoundStats) -> f64>,
) {
    card(ui, |ui| {
        ui.label(RichText::new(title).strong());
        let mut plot = Plot::new(id)
            .height(height)
            .legend(Legend::default().position(Corner::LeftTop))
            .x_axis_label("tour")
            .link_axis("stats-x", [true, false])
            .link_cursor("stats-x", [true, false])
            .allow_scroll(false)
            .include_x(0.0);
        let y_of = move |v: f64| match scale {
            Scale::Money => v,
            Scale::Percent => v * 100.0,
        };
        plot = match scale {
            Scale::Money => plot
                .y_axis_formatter(|m, _| axis_eur(m.value))
                .y_grid_spacer(decade_spacer)
                .y_axis_min_width(56.0)
                .label_formatter(|name, p| format!("{name}\ntour {:.0}\n{}", p.x, fmt_eur_l10(p.y))),
            Scale::Percent => plot
                .y_axis_formatter(|m, _| format!("{:.0} %", m.value))
                .include_y(0.0)
                .label_formatter(|name, p| format!("{name}\ntour {:.0}\n{:.1} %", p.x, p.y)),
        };
        plot.show(ui, |pu| {
            for r in runs {
                let c = palette(r.color);
                let pts: PlotPoints = r.series.iter().map(|s| [s.round as f64, y_of(f(s))]).collect();
                pu.line(Line::new(r.name.clone(), pts).color(c).width(2.0));
                if let Some(t) = theo {
                    let pts: PlotPoints = r.series.iter().map(|s| [s.round as f64, y_of(t(s))]).collect();
                    pu.line(
                        Line::new(format!("{} (théorie)", r.name), pts)
                            .color(c.gamma_multiply(0.6))
                            .style(LineStyle::dashed_dense())
                            .width(1.2),
                    );
                }
            }
        });
    });
}
