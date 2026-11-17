//! Moteur de simulation « Double ou Moitié ».
//!
//! Chaque joueur part avec `start` €, mise une part `bet_frac` de son capital à chaque tour :
//! gagné (proba `p_win`) → la mise est multipliée par `win_mult`, perdu → par `loss_mult`.
//! Jeu de base : on mise tout, ×2 ou ×0,5. Un joueur dont le capital tombe au seuil de ruine
//! (1 centime par défaut) ne peut plus jouer.
//!
//! Les capitaux sont stockés en logarithme naturel : après 1 000 tours gagnés on dépasse 10^300 €,
//! ce que f64 ne sait pas représenter directement.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Profondeur maximale de l'arbre binaire suivi (2^10 = 1 024 feuilles).
pub const TREE_MAX: usize = 10;
/// Profondeur maximale du treillis suivi.
pub const LATTICE_MAX: usize = 40;
/// Taille des paquets de joueurs traités par un même thread.
const CHUNK: usize = 16_384;
/// Nombre de classes pour le calcul des quantiles.
const QBINS: usize = 4_096;
const EPS: f64 = 1e-9;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub n_sims: usize,
    pub n_rounds: usize,
    pub start: f64,
    pub p_win: f64,
    pub win_mult: f64,
    pub loss_mult: f64,
    pub bet_frac: f64,
    pub ruin: f64,
    pub seed: u64,
    pub n_tracked: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n_sims: 10_000,
            n_rounds: 100,
            start: 1.0,
            p_win: 0.5,
            win_mult: 2.0,
            loss_mult: 0.5,
            bet_frac: 1.0,
            ruin: 0.01,
            seed: 42,
            n_tracked: 60,
        }
    }
}

impl Config {
    /// Facteur appliqué au capital total quand on gagne.
    pub fn up(&self) -> f64 {
        1.0 + self.bet_frac * (self.win_mult - 1.0)
    }
    /// Facteur appliqué au capital total quand on perd.
    pub fn down(&self) -> f64 {
        1.0 - self.bet_frac * (1.0 - self.loss_mult)
    }
    /// Espérance du facteur sur un tour (moyenne arithmétique).
    pub fn expected_factor(&self) -> f64 {
        self.p_win * self.up() + (1.0 - self.p_win) * self.down()
    }
    /// Facteur de croissance du joueur « typique » (moyenne géométrique).
    pub fn geometric_factor(&self) -> f64 {
        (self.p_win * self.up().ln() + (1.0 - self.p_win) * self.down().ln()).exp()
    }
    /// Capital (ln) d'un joueur ayant gagné `w` tours sur `d`.
    pub fn lc(&self, d: usize, w: usize) -> f64 {
        self.start.ln() + w as f64 * self.up().ln() + (d - w) as f64 * self.down().ln()
    }
    pub fn is_ruined_lc(&self, lc: f64) -> bool {
        lc <= self.ruin.ln() + EPS
    }
    pub fn short_label(&self) -> String {
        let mut s = format!(
            "{} parties · {} tours · p={} · ×{}/×{}",
            crate::util::fmt_int(self.n_sims as u64),
            self.n_rounds,
            crate::util::fmt_pct(self.p_win),
            crate::util::fmt_num(self.win_mult),
            crate::util::fmt_num(self.loss_mult),
        );
        if (self.bet_frac - 1.0).abs() > 1e-9 {
            s += &format!(" · mise {}", crate::util::fmt_pct(self.bet_frac));
        }
        s
    }
}

/// Statistiques agrégées après un tour. Les montants sont en log10(€).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RoundStats {
    pub round: u32,
    pub mean: f64,
    pub median: f64,
    pub p10: f64,
    pub p25: f64,
    pub p75: f64,
    pub p90: f64,
    pub min: f64,
    pub max: f64,
    pub ruined: f64,
    pub winners: f64,
    pub theo_mean: f64,
    pub theo_median: f64,
    pub theo_ruined: f64,
    pub theo_winners: f64,
}

#[derive(Clone, Copy)]
struct Player {
    lc: f64,
    wins: u32,
    path: u32,
    alive: bool,
    /// Résultat du dernier tour : 1 gagné, -1 perdu, 0 n'a pas joué.
    last: i8,
}

/// Calcul exact de la distribution théorique, tour par tour, sur le treillis (d, w).
#[derive(Clone)]
struct Theory {
    cfg: Config,
    d: usize,
    /// ln(proba) des nœuds encore en jeu, indexé par le nombre de victoires.
    lp: Vec<f64>,
    /// (ln capital, ln proba) des joueurs ruinés, regroupés par valeur.
    ruined: Vec<(f64, f64)>,
    /// Masse ruinée à ce tour précis (proba, pas log).
    pub newly_ruined: f64,
}

fn lae(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        return b;
    }
    if b == f64::NEG_INFINITY {
        return a;
    }
    let m = a.max(b);
    m + ((a - m).exp() + (b - m).exp()).ln()
}

impl Theory {
    fn new(cfg: &Config) -> Self {
        Self { cfg: cfg.clone(), d: 0, lp: vec![0.0], ruined: Vec::new(), newly_ruined: 0.0 }
    }

    fn step(&mut self) {
        let d = self.d + 1;
        let lpw = self.cfg.p_win.ln();
        let lpl = (1.0 - self.cfg.p_win).ln();
        let mut next = vec![f64::NEG_INFINITY; d + 1];
        let mut newly = f64::NEG_INFINITY;
        for (w, slot) in next.iter_mut().enumerate() {
            let a = if w >= 1 { self.lp[w - 1] + lpw } else { f64::NEG_INFINITY };
            let b = if w < d { self.lp[w] + lpl } else { f64::NEG_INFINITY };
            let m = lae(a, b);
            if m == f64::NEG_INFINITY || m.is_nan() {
                continue;
            }
            let lc = self.cfg.lc(d, w);
            if self.cfg.is_ruined_lc(lc) {
                newly = lae(newly, m);
                match self.ruined.iter_mut().find(|(v, _)| (v - lc).abs() < 1e-9) {
                    Some(e) => e.1 = lae(e.1, m),
                    None => self.ruined.push((lc, m)),
                }
            } else {
                *slot = m;
            }
        }
        self.lp = next;
        self.d = d;
        self.newly_ruined = newly.exp();
    }

    /// (moyenne log10, médiane log10, fraction ruinée, fraction gagnante)
    fn stats(&self) -> (f64, f64, f64, f64) {
        let lstart = self.cfg.start.ln();
        let mut lsum = f64::NEG_INFINITY;
        let mut winners = 0.0;
        let mut ruined_l = f64::NEG_INFINITY;
        let mut entries: Vec<(f64, f64)> = Vec::with_capacity(self.lp.len() + self.ruined.len());
        for &(lc, lm) in &self.ruined {
            lsum = lae(lsum, lm + lc);
            ruined_l = lae(ruined_l, lm);
            entries.push((lc, lm.exp()));
        }
        entries.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (w, &lm) in self.lp.iter().enumerate() {
            if lm == f64::NEG_INFINITY {
                continue;
            }
            let lc = self.cfg.lc(self.d, w);
            lsum = lae(lsum, lm + lc);
            if lc > lstart + EPS {
                winners += lm.exp();
            }
            entries.push((lc, lm.exp()));
        }
        // le capital croît avec w (up >= down) : `entries` est déjà trié
        let total: f64 = entries.iter().map(|e| e.1).sum();
        let mut acc = 0.0;
        let mut median = lstart;
        for (lc, m) in &entries {
            acc += m;
            if acc >= 0.5 * total {
                median = *lc;
                break;
            }
        }
        let ln10 = std::f64::consts::LN_10;
        (lsum / ln10, median / ln10, ruined_l.exp(), winners)
    }
}

/// Probabilités théoriques des nœuds du treillis : (proba[d][w] en jeu, proba ruinée au tour d).
pub fn theo_lattice(cfg: &Config, depth: usize) -> (Vec<Vec<f64>>, Vec<f64>) {
    let mut th = Theory::new(cfg);
    let mut lat = vec![vec![1.0]];
    let mut ruined = vec![0.0];
    for _ in 0..depth {
        th.step();
        lat.push(th.lp.iter().map(|l| l.exp()).collect());
        ruined.push(th.newly_ruined);
    }
    (lat, ruined)
}

/// Courbes théoriques tour par tour : [moyenne log10, médiane log10, fraction ruinée, fraction gagnante].
pub fn theo_curve(cfg: &Config, rounds: usize) -> Vec<[f64; 4]> {
    let mut th = Theory::new(cfg);
    let mut out = Vec::with_capacity(rounds + 1);
    let (a, b, c, d) = th.stats();
    out.push([a, b, c, d]);
    for _ in 0..rounds {
        th.step();
        let (a, b, c, d) = th.stats();
        out.push([a, b, c, d]);
    }
    out
}

/// Part du capital à miser qui maximise la croissance à long terme (critère de Kelly), et ce taux.
pub fn kelly(cfg: &Config) -> (f64, f64) {
    let g = cfg.win_mult - 1.0;
    let l = 1.0 - cfg.loss_mult;
    let p = cfg.p_win;
    let f = if g <= 0.0 || l <= 0.0 { 0.0 } else { (p / l - (1.0 - p) / g).clamp(0.0, 1.0) };
    (f, growth(cfg, f))
}

/// Croissance logarithmique moyenne par tour en misant la part `f`.
pub fn growth(cfg: &Config, f: f64) -> f64 {
    let up = 1.0 + f * (cfg.win_mult - 1.0);
    let down = 1.0 - f * (1.0 - cfg.loss_mult);
    cfg.p_win * up.ln() + (1.0 - cfg.p_win) * down.ln()
}

/// Accumulateur parallèle d'un tour.
#[derive(Clone)]
struct Acc {
    min: f64,
    max: f64,
    lse_m: f64,
    lse_s: f64,
    ruined: u64,
    winners: u64,
    newly_ruined: u64,
    lat: Vec<u64>,
    tree: Vec<u64>,
}

impl Acc {
    fn new(lat_len: usize, tree_len: usize) -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            lse_m: f64::NEG_INFINITY,
            lse_s: 0.0,
            ruined: 0,
            winners: 0,
            newly_ruined: 0,
            lat: vec![0; lat_len],
            tree: vec![0; tree_len],
        }
    }
    fn merge(mut self, o: Self) -> Self {
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
        if o.lse_m != f64::NEG_INFINITY {
            if self.lse_m == f64::NEG_INFINITY {
                self.lse_m = o.lse_m;
                self.lse_s = o.lse_s;
            } else {
                let m = self.lse_m.max(o.lse_m);
                self.lse_s = self.lse_s * (self.lse_m - m).exp() + o.lse_s * (o.lse_m - m).exp();
                self.lse_m = m;
            }
        }
        self.ruined += o.ruined;
        self.winners += o.winners;
        self.newly_ruined += o.newly_ruined;
        for (a, b) in self.lat.iter_mut().zip(o.lat) {
            *a += b;
        }
        for (a, b) in self.tree.iter_mut().zip(o.tree) {
            *a += b;
        }
        self
    }
    fn add_value(&mut self, lc: f64) {
        self.min = self.min.min(lc);
        self.max = self.max.max(lc);
        if lc > self.lse_m {
            self.lse_s = self.lse_s * (self.lse_m - lc).exp() + 1.0;
            self.lse_m = lc;
        } else {
            self.lse_s += (lc - self.lse_m).exp();
        }
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub struct Simulation {
    pub cfg: Config,
    players: Vec<Player>,
    pub round: usize,
    pub history: Vec<RoundStats>,
    /// log10 du capital par tour, pour les joueurs suivis individuellement.
    pub tracked: Vec<Vec<f32>>,
    /// tree[d][chemin] : joueurs ayant joué le tour d avec ce chemin (bit i = tour i+1 gagné).
    pub tree: Vec<Vec<u64>>,
    /// lattice[d][w] : joueurs encore en jeu après d tours avec w victoires.
    pub lattice: Vec<Vec<u64>>,
    /// Joueurs ruinés exactement au tour d.
    pub lattice_ruined: Vec<u64>,
    /// Distribution actuelle : (log10 €, nombre de joueurs).
    pub hist: Vec<(f64, u64)>,
    pub alive: u64,
    pub compute_secs: f64,
    /// Partie arrêtée à la main avant la fin.
    pub stopped: bool,
    theory: Theory,
}

impl Simulation {
    pub fn new(cfg: Config) -> Self {
        let lstart = cfg.start.ln();
        let n = cfg.n_sims.max(1);
        let players =
            vec![Player { lc: lstart, wins: 0, path: 0, alive: true, last: 0 }; n];
        let n_tracked = cfg.n_tracked.min(n);
        let l10 = cfg.start.log10() as f32;

        let theory = Theory::new(&cfg);
        let (tm, tmed, tr, tw) = theory.stats();
        let s0 = cfg.start.log10();
        let r0 = RoundStats {
            round: 0,
            mean: s0,
            median: s0,
            p10: s0,
            p25: s0,
            p75: s0,
            p90: s0,
            min: s0,
            max: s0,
            ruined: 0.0,
            winners: 0.0,
            theo_mean: tm,
            theo_median: tmed,
            theo_ruined: tr,
            theo_winners: tw,
        };
        Self {
            players,
            round: 0,
            history: vec![r0],
            tracked: vec![vec![l10]; n_tracked],
            tree: vec![vec![n as u64]],
            lattice: vec![vec![n as u64]],
            lattice_ruined: vec![0],
            hist: vec![(s0, n as u64)],
            alive: n as u64,
            compute_secs: 0.0,
            stopped: false,
            theory,
            cfg,
        }
    }

    pub fn n(&self) -> usize {
        self.players.len()
    }

    pub fn finished(&self) -> bool {
        self.stopped || self.round >= self.cfg.n_rounds || self.alive == 0
    }

    pub fn progress(&self) -> f32 {
        if self.alive == 0 || self.stopped {
            1.0
        } else {
            self.round as f32 / self.cfg.n_rounds.max(1) as f32
        }
    }

    pub fn last(&self) -> &RoundStats {
        self.history.last().expect("au moins le tour 0")
    }

    /// Dernier résultat et capital (log10) des joueurs suivis.
    pub fn tracked_state(&self) -> impl Iterator<Item = (i8, f64, bool)> + '_ {
        self.players[..self.tracked.len()]
            .iter()
            .map(|p| (p.last, p.lc / std::f64::consts::LN_10, p.alive))
    }

    /// Joue un tour pour tout le monde. Renvoie false si la partie était déjà finie.
    pub fn step(&mut self) -> bool {
        if self.finished() {
            return false;
        }
        let t0 = std::time::Instant::now();
        let r = self.round + 1;
        let lu = self.cfg.up().ln();
        let ld = self.cfg.down().ln();
        let lruin = self.cfg.ruin.ln() + EPS;
        let lstart = self.cfg.start.ln();
        let p = self.cfg.p_win;
        let seed = splitmix(self.cfg.seed ^ splitmix(r as u64));
        let lat_len = if r <= LATTICE_MAX { r + 1 } else { 0 };
        let tree_len = if r <= TREE_MAX { 1 << r } else { 0 };

        let acc = self
            .players
            .par_chunks_mut(CHUNK)
            .enumerate()
            .map(|(ci, chunk)| {
                let mut rng = fastrand::Rng::with_seed(splitmix(seed ^ (ci as u64).wrapping_mul(0xA24B_AED4_963E_E407)));
                let mut acc = Acc::new(lat_len, tree_len);
                for pl in chunk.iter_mut() {
                    if pl.alive {
                        if rng.f64() < p {
                            pl.lc += lu;
                            pl.wins += 1;
                            if r <= 32 {
                                pl.path |= 1 << (r - 1);
                            }
                            pl.last = 1;
                        } else {
                            pl.lc += ld;
                            pl.last = -1;
                        }
                        if pl.lc <= lruin {
                            pl.alive = false;
                            acc.newly_ruined += 1;
                        } else if lat_len > 0 {
                            acc.lat[pl.wins as usize] += 1;
                        }
                        if tree_len > 0 {
                            acc.tree[pl.path as usize] += 1;
                        }
                    } else {
                        pl.last = 0;
                    }
                    if !pl.alive {
                        acc.ruined += 1;
                    } else if pl.lc > lstart + EPS {
                        acc.winners += 1;
                    }
                    acc.add_value(pl.lc);
                }
                acc
            })
            .reduce(|| Acc::new(lat_len, tree_len), Acc::merge);

        let n = self.players.len() as f64;
        let ln10 = std::f64::consts::LN_10;

        // quantiles exacts sur le treillis : chaque classe garde la plus petite valeur rencontrée
        let (lo, hi) = (acc.min, acc.max);
        let width = ((hi - lo) / QBINS as f64).max(1e-12);
        let bins = self
            .players
            .par_chunks(CHUNK)
            .map(|chunk| {
                let mut b = vec![(0u64, f64::INFINITY); QBINS];
                for pl in chunk {
                    let i = (((pl.lc - lo) / width) as usize).min(QBINS - 1);
                    b[i].0 += 1;
                    b[i].1 = b[i].1.min(pl.lc);
                }
                b
            })
            .reduce(
                || vec![(0u64, f64::INFINITY); QBINS],
                |mut a, b| {
                    for (x, y) in a.iter_mut().zip(b) {
                        x.0 += y.0;
                        x.1 = x.1.min(y.1);
                    }
                    a
                },
            );
        let filled: Vec<(f64, u64)> =
            bins.iter().filter(|b| b.0 > 0).map(|b| (b.1 / ln10, b.0)).collect();
        let q = |f: f64| -> f64 {
            let target = (f * n).ceil().max(1.0) as u64;
            let mut c = 0;
            for &(v, k) in &filled {
                c += k;
                if c >= target {
                    return v;
                }
            }
            filled.last().map_or(0.0, |x| x.0)
        };

        self.theory.step();
        let (tm, tmed, tr, tw) = self.theory.stats();
        let mean_ln = acc.lse_m + (acc.lse_s / n).ln();
        let stats = RoundStats {
            round: r as u32,
            mean: mean_ln / ln10,
            median: q(0.5),
            p10: q(0.1),
            p25: q(0.25),
            p75: q(0.75),
            p90: q(0.9),
            min: lo / ln10,
            max: hi / ln10,
            ruined: acc.ruined as f64 / n,
            winners: acc.winners as f64 / n,
            theo_mean: tm,
            theo_median: tmed,
            theo_ruined: tr,
            theo_winners: tw,
        };

        // distribution affichée : regroupée si trop de valeurs distinctes
        self.hist = if filled.len() <= 120 {
            filled
        } else {
            let (a, b) = (lo / ln10, hi / ln10);
            let w = (b - a) / 100.0;
            let mut h = vec![0u64; 101];
            for &(v, k) in &filled {
                h[(((v - a) / w) as usize).min(100)] += k;
            }
            h.iter()
                .enumerate()
                .filter(|(_, k)| **k > 0)
                .map(|(i, k)| (a + (i as f64 + 0.5) * w, *k))
                .collect()
        };

        if lat_len > 0 {
            self.lattice.push(acc.lat);
            self.lattice_ruined.push(acc.newly_ruined);
        }
        if tree_len > 0 {
            self.tree.push(acc.tree);
        }
        for (traj, pl) in self.tracked.iter_mut().zip(&self.players) {
            traj.push((pl.lc / ln10) as f32);
        }
        self.alive = self.players.len() as u64 - acc.ruined;
        self.history.push(stats);
        self.round = r;
        self.compute_secs += t0.elapsed().as_secs_f64();
        true
    }

    pub fn to_record(&self, id: u64, name: String, color: usize) -> RunRecord {
        // on garde au plus ~1 500 points par série pour que l'historique reste léger
        let stride = (self.history.len() / 1_500).max(1);
        let mut series: Vec<RoundStats> = self.history.iter().step_by(stride).copied().collect();
        if series.last().map(|s| s.round) != Some(self.last().round) {
            series.push(*self.last());
        }
        RunRecord {
            id,
            name,
            color,
            config: self.cfg.clone(),
            created: crate::util::now_secs(),
            compute_secs: self.compute_secs,
            rounds_played: self.round,
            final_stats: *self.last(),
            series,
            final_hist: self.hist.clone(),
            selected: true,
        }
    }
}

/// Partie terminée, conservée dans l'historique pour comparaison.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RunRecord {
    pub id: u64,
    pub name: String,
    pub color: usize,
    pub config: Config,
    pub created: u64,
    pub compute_secs: f64,
    pub rounds_played: usize,
    pub final_stats: RoundStats,
    pub series: Vec<RoundStats>,
    pub final_hist: Vec<(f64, u64)>,
    pub selected: bool,
}

impl Default for RunRecord {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            color: 0,
            config: Config::default(),
            created: 0,
            compute_secs: 0.0,
            rounds_played: 0,
            final_stats: RoundStats::default(),
            series: Vec::new(),
            final_hist: Vec::new(),
            selected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jeu_de_base_ruine_apres_sept_pertes_nettes() {
        let cfg = Config::default();
        // 0,5^6 = 0,0156 € > 1 centime : encore en jeu ; 0,5^7 = 0,0078 € : ruiné
        assert!(!cfg.is_ruined_lc(cfg.lc(6, 0)));
        assert!(cfg.is_ruined_lc(cfg.lc(7, 0)));
    }

    #[test]
    fn theorie_esperance_sans_ruine() {
        let cfg = Config { ruin: 1e-300, ..Config::default() };
        let mut th = Theory::new(&cfg);
        for _ in 0..20 {
            th.step();
        }
        let (mean, median, ruined, _) = th.stats();
        assert!((mean - 20.0 * 1.25f64.log10()).abs() < 1e-9, "espérance 1,25^20");
        assert!(median.abs() < 1e-9, "médiane = 1 €");
        assert!(ruined < 1e-12);
    }

    #[test]
    fn simulation_coherente_avec_la_theorie() {
        let cfg = Config { n_sims: 200_000, n_rounds: 12, ..Config::default() };
        let mut sim = Simulation::new(cfg);
        while sim.step() {}
        let s = sim.last();
        assert!((s.ruined - s.theo_ruined).abs() < 0.01, "ruinés {} vs {}", s.ruined, s.theo_ruined);
        assert!((s.winners - s.theo_winners).abs() < 0.01);
        assert!((s.mean - s.theo_mean).abs() < 0.05, "moyenne {} vs {}", s.mean, s.theo_mean);
        // le treillis compte tout le monde
        let tot: u64 = sim.lattice[12].iter().sum::<u64>() + sim.lattice_ruined.iter().sum::<u64>();
        assert_eq!(tot, 200_000);
    }

    #[test]
    fn meme_graine_meme_resultat() {
        let cfg = Config { n_sims: 50_000, n_rounds: 30, ..Config::default() };
        let mut a = Simulation::new(cfg.clone());
        let mut b = Simulation::new(cfg);
        while a.step() {}
        while b.step() {}
        assert_eq!(a.last().mean, b.last().mean);
    }
}

#[cfg(test)]
mod bench {
    use super::*;

    #[test]
    #[ignore]
    fn vitesse() {
        for (n, t) in [(1_000_000, 200), (2_000_000, 100), (10_000, 5_000)] {
            let cfg = Config { n_sims: n, n_rounds: t, ruin: 1e-300, ..Config::default() };
            let mut sim = Simulation::new(cfg);
            let t0 = std::time::Instant::now();
            while sim.step() {}
            let s = t0.elapsed().as_secs_f64();
            println!("{n} joueurs × {t} tours : {:.2} s ({:.0} M parties-tours/s)", s, (n * t) as f64 / s / 1e6);
        }
    }
}
