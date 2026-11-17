use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::app::{App, TreeMode};
use crate::sim::{Config, LATTICE_MAX, Simulation, TREE_MAX, theo_lattice};
use crate::util::*;

/// Informations affichées au survol d'un nœud.
struct Hover {
    pos: Pos2,
    lines: Vec<(String, Color32)>,
}

impl App {
    pub fn tree_tab(&mut self, ui: &mut egui::Ui) {
        let App { sim, view, cfg, .. } = self;
        let sim = sim.as_ref();
        let cfg = sim.map_or_else(|| cfg.clone(), |s| s.cfg.clone());

        ui.horizontal(|ui| {
            ui.label(RichText::new("Arbre des probabilités").size(18.0).strong());
            ui.add_space(16.0);
            ui.selectable_value(&mut view.tree_mode, TreeMode::Binary, "🌳 Arbre complet");
            ui.selectable_value(&mut view.tree_mode, TreeMode::Lattice, "🔷 Treillis (chemins regroupés)");
            ui.add_space(16.0);
            match view.tree_mode {
                TreeMode::Binary => {
                    ui.add(egui::Slider::new(&mut view.tree_depth, 1..=TREE_MAX).text("tours"));
                }
                TreeMode::Lattice => {
                    ui.add(egui::Slider::new(&mut view.lattice_depth, 1..=LATTICE_MAX).text("tours"));
                }
            }
            ui.checkbox(&mut view.tree_labels, "Montants et pourcentages");
        });
        ui.horizontal_wrapped(|ui| {
            legend_dot(ui, GREEN, "plus que la mise de départ");
            legend_dot(ui, MUTED, "à l'équilibre");
            legend_dot(ui, RED, "moins que la mise");
            legend_dot(ui, DARK_RED, "ruiné (ne peut plus jouer)");
            ui.label(
                RichText::new(match sim {
                    Some(s) => format!(
                        "· taille et épaisseur = part réelle des {} joueurs simulés (cercle vide : théorie, pas encore joué)",
                        fmt_int(s.n() as u64)
                    ),
                    None => "· taille et épaisseur = probabilité théorique (lance une simulation pour voir les vrais joueurs)"
                        .into(),
                })
                .small()
                .color(MUTED),
            );
        });
        ui.add_space(6.0);

        let size = ui.available_size();
        let (resp, painter) = ui.allocate_painter(size, Sense::hover());
        painter.rect_filled(resp.rect, 10, CARD);
        let rect = resp.rect.shrink2(Vec2::new(70.0, 34.0));
        let pointer = resp.hover_pos();
        let hover = match view.tree_mode {
            TreeMode::Binary => draw_tree(&painter, rect, &cfg, sim, view.tree_depth, view.tree_labels, pointer),
            TreeMode::Lattice => draw_lattice(&painter, rect, &cfg, sim, view.lattice_depth, view.tree_labels, pointer),
        };
        if let Some(h) = hover {
            painter.circle_stroke(h.pos, 9.0, Stroke::new(2.0, Color32::WHITE));
            resp.on_hover_ui_at_pointer(|ui| {
                for (t, c) in &h.lines {
                    ui.label(RichText::new(t).color(*c));
                }
            });
        }
    }
}

fn legend_dot(ui: &mut egui::Ui, c: Color32, label: &str) {
    crate::app::dot(ui, c, 6.0);
    ui.label(RichText::new(label).small());
    ui.add_space(6.0);
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Alive,
    RuinedHere,
}

#[derive(Clone, Copy)]
struct Node {
    idx: usize,
    w: usize,
    path: u32,
    status: Status,
    theo: f64,
    pos: Pos2,
}

fn observed_label(count: Option<u64>, n: f64) -> String {
    match count {
        Some(k) => format!("Observé : {} joueurs ({})", fmt_int(k), fmt_pct(k as f64 / n)),
        None => "Observé : tour pas encore joué".into(),
    }
}

fn draw_tree(
    painter: &egui::Painter,
    rect: Rect,
    cfg: &Config,
    sim: Option<&Simulation>,
    depth: usize,
    labels: bool,
    pointer: Option<Pos2>,
) -> Option<Hover> {
    let depth = depth.min(TREE_MAX);
    let n = sim.map_or(1.0, |s| s.n() as f64);
    let start_l10 = cfg.start.log10();
    let ln10 = std::f64::consts::LN_10;
    let x = |d: usize| rect.left() + rect.width() * d as f32 / depth as f32;
    let font = FontId::proportional(12.0);
    let mut best: Option<(f32, Hover)> = None;

    let obs = |d: usize, path: u32| -> Option<u64> {
        sim.and_then(|s| s.tree.get(d)).map(|lvl| lvl[path as usize])
    };

    let mut level = vec![Node {
        idx: 0,
        w: 0,
        path: 0,
        status: Status::Alive,
        theo: 1.0,
        pos: Pos2::new(x(0), rect.center().y),
    }];
    let mut levels = vec![level.clone()];
    for d in 1..=depth {
        let slots = 1usize << d;
        let spacing = rect.height() / slots as f32;
        let mut next = Vec::with_capacity(level.len() * 2);
        for parent in &level {
            if parent.status != Status::Alive {
                continue;
            }
            for win in [true, false] {
                let idx = parent.idx * 2 + usize::from(!win);
                let w = parent.w + usize::from(win);
                let path = parent.path | (u32::from(win) << (d - 1));
                let lc = cfg.lc(d, w);
                let status = if cfg.is_ruined_lc(lc) { Status::RuinedHere } else { Status::Alive };
                let theo = parent.theo * if win { cfg.p_win } else { 1.0 - cfg.p_win };
                let pos = Pos2::new(x(d), rect.top() + (idx as f32 + 0.5) * spacing);
                next.push(Node { idx, w, path, status, theo, pos });
            }
        }
        levels.push(next.clone());
        level = next;
    }

    // en-têtes de colonnes
    for d in 0..=depth {
        let label = if d == 0 { "Départ".to_string() } else { format!("Tour {d}") };
        painter.text(Pos2::new(x(d), rect.top() - 18.0), Align2::CENTER_CENTER, label, font.clone(), MUTED);
    }

    // arêtes puis nœuds, niveau par niveau
    for d in 1..=depth {
        let parents = &levels[d - 1];
        let children = &levels[d];
        let mut ci = 0;
        for parent in parents.iter().filter(|p| p.status == Status::Alive) {
            for _ in 0..2 {
                let child = &children[ci];
                ci += 1;
                let frac = obs(d, child.path).map_or(child.theo, |k| k as f64 / n);
                let win = child.w > parent.w;
                let base = if win { GREEN } else { RED };
                let width = 0.6 + 7.0 * frac.sqrt() as f32;
                let alpha = 0.15 + 0.75 * frac.sqrt() as f32;
                painter.line_segment([parent.pos, child.pos], Stroke::new(width, base.gamma_multiply(alpha)));
            }
        }
    }
    for d in 0..=depth {
        let spacing = rect.height() / (1usize << d) as f32;
        let max_r = (spacing * 0.45).clamp(1.5, 16.0);
        for node in &levels[d] {
            let lc = cfg.lc(d, node.w);
            let l10 = lc / ln10;
            let ruined = node.status == Status::RuinedHere;
            let count = obs(d, node.path);
            let frac = count.map_or(node.theo, |k| k as f64 / n);
            let r = (3.0 + 14.0 * frac.sqrt() as f32).min(max_r);
            let color = money_color(l10, start_l10, ruined);
            if count == Some(0) || (count.is_none() && sim.is_some()) {
                painter.circle_stroke(node.pos, r, Stroke::new(1.2, color));
            } else {
                painter.circle_filled(node.pos, r, color);
            }
            if ruined && spacing > 10.0 {
                painter.text(node.pos, Align2::CENTER_CENTER, "✖", FontId::proportional(r.max(9.0)), Color32::WHITE);
            }
            if labels && spacing >= 30.0 {
                let tx = node.pos + Vec2::new(r + 5.0, -7.0);
                painter.text(tx, Align2::LEFT_CENTER, fmt_eur_l10(l10), FontId::proportional(13.0), color);
                let pct = match count {
                    Some(k) => format!("{} (théo {})", fmt_pct(k as f64 / n), fmt_pct(node.theo)),
                    None => fmt_pct(node.theo),
                };
                painter.text(tx + Vec2::new(0.0, 14.0), Align2::LEFT_CENTER, pct, font.clone(), MUTED);
            }
            if let Some(p) = pointer {
                let dist = p.distance(node.pos);
                if dist < (r + 8.0).max(10.0) && best.as_ref().is_none_or(|(b, _)| dist < *b) {
                    let mut seq = String::new();
                    for i in 0..d {
                        seq.push(if node.path >> i & 1 == 1 { 'G' } else { 'P' });
                        seq.push(' ');
                    }
                    let mut lines = vec![
                        (format!("Après {d} tour(s)"), Color32::WHITE),
                        (
                            if d == 0 { "Point de départ".into() } else { format!("Chemin : {seq}(G = gagné, P = perdu)") },
                            MUTED,
                        ),
                        (format!("Capital : {}", fmt_eur_l10(l10)), color),
                        (format!("Probabilité théorique : {}", fmt_pct(node.theo)), Color32::WHITE),
                    ];
                    if sim.is_some() {
                        lines.push((observed_label(count, n), Color32::WHITE));
                    }
                    if ruined {
                        lines.push(("Ruiné : ce joueur ne peut plus jouer".into(), RED));
                    }
                    best = Some((dist, Hover { pos: node.pos, lines }));
                }
            }
        }
    }
    best.map(|b| b.1)
}

fn draw_lattice(
    painter: &egui::Painter,
    rect: Rect,
    cfg: &Config,
    sim: Option<&Simulation>,
    depth: usize,
    labels: bool,
    pointer: Option<Pos2>,
) -> Option<Hover> {
    let depth = depth.min(LATTICE_MAX);
    let (tl, tr) = theo_lattice(cfg, depth);
    let n = sim.map_or(1.0, |s| s.n() as f64);
    let ln10 = std::f64::consts::LN_10;
    let start_l10 = cfg.start.log10();
    let lruin = cfg.ruin.ln();
    let ld = cfg.down().ln();

    let hi = cfg.lc(depth, depth);
    let lo = (lruin + ld).max(cfg.lc(depth, 0));
    let (lo, hi) = if hi - lo < 1e-6 { (lo - 1.0, hi + 1.0) } else { (lo, hi) };
    let x = |d: usize| rect.left() + rect.width() * d as f32 / depth as f32;
    let y = |lc: f64| rect.bottom() - rect.height() * ((lc - lo) / (hi - lo)) as f32;
    let dx = rect.width() / depth as f32;
    let dy = (rect.height() as f64 * (cfg.up().ln() - ld) / (hi - lo)) as f32;
    let max_r = (dx.min(dy) * 0.42).clamp(2.0, 18.0);
    let small = FontId::proportional(11.5);
    let mut best: Option<(f32, Hover)> = None;

    let obs_alive = |d: usize, w: usize| -> Option<u64> { sim.and_then(|s| s.lattice.get(d)).map(|l| l[w]) };
    let obs_ruined = |d: usize| -> Option<u64> { sim.and_then(|s| s.lattice_ruined.get(d)).copied() };

    // repères horizontaux
    let ys = y(cfg.start.ln());
    painter.hline(rect.x_range(), ys, Stroke::new(1.0, MUTED.gamma_multiply(0.5)));
    painter.text(Pos2::new(rect.left() - 8.0, ys), Align2::RIGHT_CENTER, fmt_eur(cfg.start), small.clone(), MUTED);
    if lruin >= lo {
        let yr = y(lruin);
        painter.hline(rect.x_range(), yr, Stroke::new(1.0, RED.gamma_multiply(0.6)));
        painter.text(Pos2::new(rect.left() - 8.0, yr), Align2::RIGHT_CENTER, "ruine", small.clone(), RED);
    }
    let step = (depth / 10).max(1);
    for d in (0..=depth).step_by(step) {
        painter.text(Pos2::new(x(d), rect.top() - 18.0), Align2::CENTER_CENTER, format!("{d}"), small.clone(), MUTED);
    }
    painter.text(Pos2::new(rect.right() + 10.0, rect.top() - 18.0), Align2::LEFT_CENTER, "tours", small.clone(), MUTED);

    // arêtes
    for d in 0..depth {
        for w in 0..=d {
            if tl[d][w] <= 0.0 && obs_alive(d, w).unwrap_or(0) == 0 {
                continue;
            }
            let frac = obs_alive(d, w).map_or(tl[d][w], |k| k as f64 / n);
            let from = Pos2::new(x(d), y(cfg.lc(d, w)));
            for (win, c) in [(true, GREEN), (false, RED)] {
                let cw = w + usize::from(win);
                let to = Pos2::new(x(d + 1), y(cfg.lc(d + 1, cw)));
                let share = if win { cfg.p_win } else { 1.0 - cfg.p_win };
                let f = (frac * share).sqrt() as f32;
                painter.line_segment([from, to], Stroke::new(0.5 + 6.0 * f, c.gamma_multiply(0.12 + 0.6 * f)));
            }
        }
    }

    // nœuds
    for d in 0..=depth {
        for w in 0..=d {
            let lc = cfg.lc(d, w);
            let ruined = cfg.is_ruined_lc(lc);
            let (theo, count) = if ruined {
                // ruiné à ce tour : on vient forcément d'un nœud vivant par une perte
                if d == 0 || w >= tl[d - 1].len() || tl[d - 1][w] <= 0.0 {
                    continue;
                }
                (tr[d], obs_ruined(d))
            } else {
                if tl[d][w] <= 0.0 {
                    continue;
                }
                (tl[d][w], obs_alive(d, w))
            };
            let pos = Pos2::new(x(d), y(lc));
            let l10 = lc / ln10;
            let color = money_color(l10, start_l10, ruined);
            let frac = count.map_or(theo, |k| k as f64 / n);
            let r = (2.5 + 16.0 * frac.sqrt() as f32).min(max_r);
            if count == Some(0) || (count.is_none() && sim.is_some()) {
                painter.circle_stroke(pos, r, Stroke::new(1.2, color));
            } else {
                painter.circle_filled(pos, r, color);
            }
            if ruined {
                painter.text(pos, Align2::CENTER_CENTER, "✖", FontId::proportional(r.max(9.0)), Color32::WHITE);
            }
            if labels && dx >= 46.0 && dy >= 30.0 {
                painter.text(pos + Vec2::new(0.0, -r - 8.0), Align2::CENTER_CENTER, fmt_eur_l10(l10), small.clone(), color);
                let pct = fmt_pct(frac);
                painter.text(pos + Vec2::new(0.0, r + 8.0), Align2::CENTER_CENTER, pct, small.clone(), MUTED);
            }
            if let Some(p) = pointer {
                let dist = p.distance(pos);
                if dist < (r + 8.0).max(10.0) && best.as_ref().is_none_or(|(b, _)| dist < *b) {
                    let paths = if ruined {
                        None
                    } else {
                        let per_path = cfg.p_win.powi(w as i32) * (1.0 - cfg.p_win).powi((d - w) as i32);
                        (per_path > 0.0).then(|| (theo / per_path).round())
                    };
                    let mut lines = vec![
                        (format!("Après {d} tour(s) : {w} gagné(s), {} perdu(s)", d - w), Color32::WHITE),
                        (format!("Capital : {}", fmt_eur_l10(l10)), color),
                        (format!("Probabilité théorique : {}", fmt_pct(theo)), Color32::WHITE),
                    ];
                    if let Some(k) = paths
                        && k < 1e15
                    {
                        lines.push((format!("Chemins qui mènent ici sans ruine : {}", fmt_int(k as u64)), MUTED));
                    }
                    if sim.is_some() {
                        lines.push((observed_label(count, n), Color32::WHITE));
                    }
                    if ruined {
                        lines.push((format!("Ruinés exactement au tour {d}"), RED));
                    }
                    best = Some((dist, Hover { pos, lines }));
                }
            }
        }
    }
    best.map(|b| b.1)
}
