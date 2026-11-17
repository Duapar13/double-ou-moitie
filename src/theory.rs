use eframe::egui::{self, Color32, RichText};
use egui_plot::{Corner, Legend, Line, LineStyle, Plot, PlotPoints, Points, VLine};

use crate::app::{App, card};
use crate::sim::{Config, growth, kelly, theo_curve};
use crate::util::*;

const THEORY_MAX_ROUNDS: usize = 1_000;

/// Les champs qui changent la théorie (pas le nombre de joueurs ni la graine).
fn theory_key(c: &Config) -> Config {
    Config { n_sims: 0, seed: 0, n_tracked: 0, ..c.clone() }
}

impl App {
    pub fn theory_tab(&mut self, ui: &mut egui::Ui) {
        let cfg = self.cfg.clone();
        let rounds = cfg.n_rounds.min(THEORY_MAX_ROUNDS);
        let key = theory_key(&cfg);
        if self.theory_cache.as_ref().is_none_or(|(k, r, _)| *k != key || *r != rounds) {
            self.theory_cache = Some((key, rounds, theo_curve(&cfg, rounds)));
        }
        let curve = &self.theory_cache.as_ref().expect("rempli juste avant").2;
        let end = curve.last().copied().unwrap_or([0.0; 4]);

        let p = cfg.p_win;
        let (up, down) = (cfg.up(), cfg.down());
        let e = cfg.expected_factor();
        let g = cfg.geometric_factor();
        let (kf, kg) = kelly(&cfg);
        let mut apply_kelly = false;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new("Pourquoi la moyenne monte alors que presque tout le monde perd ?").size(22.0).strong());
            ui.label(
                RichText::new(format!(
                    "Avec les règles actuelles : {} de chances de gagner, le capital est multiplié par {} si on gagne et par {} si on perd.",
                    fmt_pct(p),
                    fmt_num(up),
                    fmt_num(down)
                ))
                .color(MUTED),
            );
            ui.add_space(10.0);

            ui.columns(2, |c| {
                explain(
                    &mut c[0],
                    "1. Ce que promet la moyenne",
                    VIOLET,
                    &format!(
                        "À chaque tour, l'espérance vaut {p} × {up} + {q} × {down} = {e}.\n\
                         Le capital moyen gagne donc {pct} par tour. Après {t} tours : {start} × {e}^{t} = {mean}.\n\n\
                         Ce chiffre est exact… mais il est porté par une poignée de chanceux. \
                         Celui qui gagne les {t} tours finit à {best}, avec une chance sur {odds}.",
                        p = fmt_num(p),
                        q = fmt_num(1.0 - p),
                        up = fmt_num(up),
                        down = fmt_num(down),
                        e = fmt_num(e),
                        pct = fmt_pct(e - 1.0),
                        t = rounds,
                        start = fmt_eur(cfg.start),
                        mean = fmt_eur_l10(cfg.start.log10() + rounds as f64 * e.log10()),
                        best = fmt_eur_l10(cfg.lc(rounds, rounds) / std::f64::consts::LN_10),
                        odds = fmt_eur_l10(-(rounds as f64) * p.log10()).trim_end_matches('€').trim_end(),
                    ),
                );
                explain(
                    &mut c[1],
                    "2. Ce que vit le joueur typique",
                    GOLD,
                    &format!(
                        "Un gain puis une perte : × {up} × {down} = × {both}. \
                         Le joueur typique ne suit pas la moyenne arithmétique mais la moyenne géométrique :\n\
                         {up}^{p} × {down}^{q} = {g} par tour.\n\n{verdict}",
                        up = fmt_num(up),
                        down = fmt_num(down),
                        both = fmt_num(up * down),
                        p = fmt_num(p),
                        q = fmt_num(1.0 - p),
                        g = fmt_num(g),
                        verdict = if (g - 1.0).abs() < 1e-9 {
                            "Elle vaut exactement 1 : sans seuil de ruine, le joueur médian resterait bloqué à sa mise de départ, \
                             pendant que la moyenne s'envole."
                                .to_string()
                        } else if g < 1.0 {
                            format!("Elle est inférieure à 1 : le joueur typique perd {} par tour, même si la moyenne monte.", fmt_pct(1.0 - g))
                        } else {
                            format!("Elle est supérieure à 1 : le joueur typique gagne {} par tour.", fmt_pct(g - 1.0))
                        }
                    ),
                );
            });
            ui.add_space(8.0);
            ui.columns(2, |c| {
                let pertes = ((cfg.ruin / cfg.start).ln() / down.ln()).ceil();
                explain(
                    &mut c[0],
                    "3. Le piège : la ruine",
                    RED,
                    &format!(
                        "Dès que le capital tombe à {ruin} ou moins, on ne peut plus jouer{pertes}. \
                         Gains et pertes se compensent, donc le capital fait une marche au hasard : \
                         tôt ou tard, presque tout le monde touche le seuil.\n\n\
                         Après {t} tours, la théorie prévoit {r} de joueurs ruinés et seulement {w} au-dessus de leur mise de départ.",
                        ruin = fmt_eur(cfg.ruin),
                        pertes = if pertes.is_finite() && pertes > 0.0 {
                            format!(" (soit {} pertes nettes d'affilée depuis {})", pertes as i64, fmt_eur(cfg.start))
                        } else {
                            String::new()
                        },
                        t = rounds,
                        r = fmt_pct(end[2]),
                        w = fmt_pct(end[3]),
                    ),
                );
                c[1].vertical(|ui| {
                    explain(
                        ui,
                        "4. La parade : ne pas tout miser (critère de Kelly)",
                        GREEN,
                        &format!(
                            "Si on ne mise qu'une partie f de son capital, la croissance du joueur typique devient \
                             p·ln(1 + f·{gm}) + (1 − p)·ln(1 − f·{lm}).\n\n\
                             Elle est maximale pour f = {kf} : le joueur typique gagne alors {kg} par tour, \
                             sans jamais risquer la ruine d'un coup. Tu mises actuellement {f}.",
                            gm = fmt_num(cfg.win_mult - 1.0),
                            lm = fmt_num(1.0 - cfg.loss_mult),
                            kf = fmt_pct(kf),
                            kg = fmt_pct(kg.exp() - 1.0),
                            f = fmt_pct(cfg.bet_frac),
                        ),
                    );
                    if kf > 0.0
                        && (kf - cfg.bet_frac).abs() > 1e-3
                        && ui.button(format!("Essayer la mise de Kelly ({})", fmt_pct(kf))).clicked()
                    {
                        apply_kelly = true;
                    }
                });
            });
            ui.add_space(10.0);

            let h = 300.0;
            ui.columns(2, |c| {
                card(&mut c[0], |ui| {
                    ui.label(RichText::new(format!("Théorie exacte sur {rounds} tours")).strong());
                    Plot::new("theo-curves")
                        .height(h)
                        .legend(Legend::default().position(Corner::LeftTop))
                        .x_axis_label("tour")
                        .y_axis_formatter(|m, _| axis_eur(m.value))
                        .y_grid_spacer(decade_spacer)
                        .y_axis_min_width(56.0)
                        .allow_scroll(false)
                        .label_formatter(|name, p| format!("{name}\ntour {:.0}\n{}", p.x, fmt_eur_l10(p.y)))
                        .show(ui, |pu| {
                            let line = |i: usize| -> PlotPoints<'static> {
                                curve.iter().enumerate().map(|(t, v)| [t as f64, v[i]]).collect()
                            };
                            pu.line(Line::new("Capital moyen", line(0)).color(VIOLET).width(2.5));
                            pu.line(Line::new("Capital médian", line(1)).color(GOLD).width(2.5));
                        });
                });
                card(&mut c[1], |ui| {
                    ui.label(RichText::new("Croissance du joueur typique selon la part misée").strong());
                    Plot::new("kelly")
                        .height(h)
                        .legend(Legend::default().position(Corner::RightTop))
                        .x_axis_formatter(|m, _| format!("{:.0} %", m.value))
                        .y_axis_formatter(|m, _| format!("{:+.1} %", m.value))
                        .x_axis_label("part du capital misée à chaque tour")
                        .allow_scroll(false)
                        .label_formatter(|_, p| format!("mise {:.0} %\n{:+.2} % par tour", p.x, p.y))
                        .show(ui, |pu| {
                            let pts: PlotPoints = (1..=100)
                                .map(|i| {
                                    let f = i as f64 / 100.0;
                                    [i as f64, (growth(&cfg, f).exp() - 1.0) * 100.0]
                                })
                                .collect();
                            pu.line(Line::new("Croissance par tour", pts).color(GREEN).width(2.5));
                            pu.hline(egui_plot::HLine::new("Zéro", 0.0).color(MUTED.gamma_multiply(0.6)).style(LineStyle::dashed_loose()));
                            pu.vline(VLine::new("Mise de Kelly", kf * 100.0).color(GREEN.gamma_multiply(0.7)).style(LineStyle::dashed_dense()));
                            pu.points(
                                Points::new("Ta mise actuelle", vec![[cfg.bet_frac * 100.0, (growth(&cfg, cfg.bet_frac).exp() - 1.0) * 100.0]])
                                    .color(Color32::WHITE)
                                    .radius(6.0),
                            );
                        });
                });
            });
        });
        if apply_kelly {
            self.cfg.bet_frac = kf.max(0.01);
        }
    }
}

fn explain(ui: &mut egui::Ui, title: &str, color: Color32, body: &str) {
    card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).size(17.0).strong().color(color));
        ui.add_space(4.0);
        ui.label(RichText::new(body).size(14.5));
    });
}
