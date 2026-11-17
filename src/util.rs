use eframe::egui::Color32;
use egui_plot::{GridInput, GridMark};

pub const GREEN: Color32 = Color32::from_rgb(52, 211, 153);
pub const RED: Color32 = Color32::from_rgb(248, 113, 113);
pub const DARK_RED: Color32 = Color32::from_rgb(127, 29, 29);
pub const GOLD: Color32 = Color32::from_rgb(251, 191, 36);
pub const BLUE: Color32 = Color32::from_rgb(96, 165, 250);
pub const VIOLET: Color32 = Color32::from_rgb(167, 139, 250);
pub const MUTED: Color32 = Color32::from_rgb(148, 163, 184);
pub const CARD: Color32 = Color32::from_rgb(30, 36, 52);
pub const CARD_STROKE: Color32 = Color32::from_rgb(51, 60, 84);

const PALETTE: [Color32; 10] = [
    Color32::from_rgb(96, 165, 250),
    Color32::from_rgb(251, 146, 60),
    Color32::from_rgb(52, 211, 153),
    Color32::from_rgb(244, 114, 182),
    Color32::from_rgb(250, 204, 21),
    Color32::from_rgb(167, 139, 250),
    Color32::from_rgb(45, 212, 191),
    Color32::from_rgb(248, 113, 113),
    Color32::from_rgb(163, 230, 53),
    Color32::from_rgb(226, 232, 240),
];

pub fn palette(i: usize) -> Color32 {
    PALETTE[i % PALETTE.len()]
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn ago(t: u64) -> String {
    let s = now_secs().saturating_sub(t);
    match s {
        0..60 => "à l'instant".into(),
        60..3_600 => format!("il y a {} min", s / 60),
        3_600..86_400 => format!("il y a {} h", s / 3_600),
        _ => format!("il y a {} j", s / 86_400),
    }
}

/// 1234567 → "1 234 567"
pub fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Nombre court à la française : 2 → "2", 0.5 → "0,5", 1.25 → "1,25"
pub fn fmt_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.replace('.', ",")
}

pub fn fmt_pct(f: f64) -> String {
    let p = f * 100.0;
    let s = if p == 0.0 || p == 100.0 || (p - p.round()).abs() < 1e-9 {
        format!("{:.0} %", p)
    } else if p.abs() < 0.1 {
        format!("{:.3} %", p)
    } else {
        format!("{:.1} %", p)
    };
    s.replace('.', ",")
}

/// Façon d'écrire les grands montants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Notation {
    /// 4,5 K€ · 4,5 M€ · 4,5 B€ · 4,5 Qa€ … puis aa, ab, ac…
    #[default]
    Suffixes,
    /// 4,5 millions € · 4,5 milliards € · 4,5 billiards € …
    Mots,
    /// 4,5 × 10^15 €
    Scientifique,
}

impl Notation {
    pub fn label(self) -> &'static str {
        match self {
            Notation::Suffixes => "Suffixes : 4,5 K€ · 4,5 M€ · 4,5 B€",
            Notation::Mots => "En mots : 4,5 millions €",
            Notation::Scientifique => "Scientifique : 4,5 × 10^6 €",
        }
    }
}

static NOTATION: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_notation(n: Notation) {
    NOTATION.store(n as u8, std::sync::atomic::Ordering::Relaxed);
}

pub fn notation() -> Notation {
    match NOTATION.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Notation::Mots,
        2 => Notation::Scientifique,
        _ => Notation::Suffixes,
    }
}

/// Suffixes « jeux idle » : un par puissance de 1 000.
const SUFFIXES: [&str; 12] = ["", "K", "M", "B", "T", "Qa", "Qi", "Sx", "Sp", "Oc", "No", "Dc"];

fn suffix(g: usize) -> String {
    if g < SUFFIXES.len() {
        return SUFFIXES[g].to_string();
    }
    // au-delà de 10^33 : aa, ab, … zz (jusqu'à 10^2061)
    let i = g - SUFFIXES.len();
    if i >= 26 * 26 {
        return String::new();
    }
    let a = (b'a' + (i / 26) as u8) as char;
    let b = (b'a' + (i % 26) as u8) as char;
    format!("{a}{b}")
}

/// Échelle longue française : un nom par puissance de 1 000, à partir du million.
const MOTS: [&str; 22] = [
    "", "mille", "million", "milliard", "billion", "billiard", "trillion", "trilliard", "quadrillion",
    "quadrilliard", "quintillion", "quintilliard", "sextillion", "sextilliard", "septillion", "septilliard",
    "octillion", "octilliard", "nonillion", "nonilliard", "décillion", "décilliard",
];

/// Mantisse à 3 chiffres significatifs, zéros inutiles retirés : 4.5 → "4,5", 450 → "450".
fn mantissa(m: f64) -> String {
    let s = if m < 10.0 {
        format!("{:.2}", m)
    } else if m < 100.0 {
        format!("{:.1}", m)
    } else {
        format!("{:.0}", m)
    };
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

/// Découpe 10^l10 en (mantisse arrondie dans [1, 1000), groupe de 3 chiffres).
fn split_thousands(l10: f64) -> (f64, usize) {
    let mut g = (l10 / 3.0).floor().max(0.0) as usize;
    let mut m = 10f64.powf(l10 - 3.0 * g as f64);
    let rounded = if m < 10.0 {
        (m * 100.0).round() / 100.0
    } else if m < 100.0 {
        (m * 10.0).round() / 10.0
    } else {
        m.round()
    };
    m = rounded;
    if m >= 1000.0 {
        m /= 1000.0;
        g += 1;
    }
    (m, g)
}

fn scientific(l10: f64) -> String {
    let mut e = l10.floor();
    let mut m = ((10f64.powf(l10 - e)) * 100.0).round() / 100.0;
    if m >= 10.0 {
        m /= 10.0;
        e += 1.0;
    }
    format!("{} × 10^{} €", fmt_num(m), e as i64)
}

/// Montant en euros à partir de son log10, dans la notation choisie.
pub fn fmt_eur_l10(l10: f64) -> String {
    fmt_eur_l10_with(l10, notation())
}

pub fn fmt_eur_l10_with(l10: f64, n: Notation) -> String {
    if !l10.is_finite() {
        return if l10 > 0.0 { "∞".into() } else { "0 €".into() };
    }
    if l10 < -4.0 {
        return scientific(l10);
    }
    match n {
        Notation::Suffixes if l10 >= 3.0 => {
            let (m, g) = split_thousands(l10);
            let s = suffix(g);
            if s.is_empty() { scientific(l10) } else { format!("{} {s}€", mantissa(m)) }
        }
        Notation::Mots if l10 >= 6.0 => {
            let (m, g) = split_thousands(l10);
            match MOTS.get(g) {
                Some(name) if g >= 2 => {
                    let plural = if m >= 2.0 { "s" } else { "" };
                    format!("{} {name}{plural} €", mantissa(m))
                }
                _ => scientific(l10),
            }
        }
        Notation::Scientifique if l10 >= 6.0 => scientific(l10),
        _ => plain_eur(10f64.powf(l10)),
    }
}

/// Les trois écritures d'un même montant (pour les infobulles).
pub fn eur_detail(l10: f64) -> String {
    let mut lines = vec![fmt_eur_l10_with(l10, Notation::Suffixes)];
    for n in [Notation::Mots, Notation::Scientifique] {
        let s = fmt_eur_l10_with(l10, n);
        if !lines.contains(&s) {
            lines.push(s);
        }
    }
    lines.join("\n= ")
}

pub fn fmt_eur(v: f64) -> String {
    if v >= 1_000.0 { fmt_eur_l10(v.log10()) } else { plain_eur(v) }
}

/// Montant écrit en entier : "0,50 €", "123,45 €", "12 345 €".
fn plain_eur(v: f64) -> String {
    if v >= 1_000.0 {
        format!("{} €", fmt_int(v.round() as u64))
    } else if v >= 0.01 - 1e-12 || v == 0.0 {
        let s = format!("{:.2}", v).replace('.', ",");
        let s = s.strip_suffix(",00").unwrap_or(&s).to_string();
        format!("{s} €")
    } else {
        format!("{} €", format!("{:.4}", v).replace('.', ","))
    }
}

/// Étiquette compacte pour un axe logarithmique.
pub fn axis_eur(l10: f64) -> String {
    let r = l10.round();
    if notation() == Notation::Scientifique && (l10 - r).abs() < 1e-6 && r >= 6.0 {
        return format!("10^{} €", r as i64);
    }
    fmt_eur_l10(l10)
}

/// Graduations d'un axe en log10 : décades entières, ou 1-2-5 quand on zoome.
pub fn decade_spacer(input: GridInput) -> Vec<GridMark> {
    let (lo, hi) = input.bounds;
    let span = (hi - lo).max(1e-9);
    let mut marks = Vec::new();
    if span <= 3.0 {
        let (a, b) = (lo.floor() as i64 - 1, hi.ceil() as i64 + 1);
        for e in a..=b {
            for (m, size) in [(1.0f64, 1.0), (2.0, 0.3), (5.0, 0.3)] {
                let v = e as f64 + m.log10();
                if v >= lo && v <= hi {
                    marks.push(GridMark { value: v, step_size: size });
                }
            }
        }
    } else {
        let mut step = 1.0;
        while span / step > 10.0 {
            step *= 2.0;
        }
        let mut v = (lo / step).floor() * step;
        while v <= hi {
            if v >= lo {
                marks.push(GridMark { value: v, step_size: step });
            }
            v += step;
        }
    }
    marks
}

/// Couleur d'un capital selon sa position par rapport à la mise de départ.
pub fn money_color(l10: f64, start_l10: f64, ruined: bool) -> Color32 {
    if ruined {
        DARK_RED
    } else if l10 > start_l10 + 1e-9 {
        GREEN
    } else if l10 < start_l10 - 1e-9 {
        RED
    } else {
        MUTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(fmt_int(1_234_567), "1 234 567");
        assert_eq!(fmt_num(0.5), "0,5");
        assert_eq!(fmt_num(2.0), "2");
        assert_eq!(fmt_eur(0.5), "0,50 €");
        assert_eq!(fmt_eur(2.0), "2 €");
        assert_eq!(fmt_eur(0.0078125), "0,0078 €");
        assert_eq!(fmt_pct(0.5), "50 %");
        assert_eq!(fmt_eur_l10(0.0), "1 €");
    }

    #[test]
    fn notations() {
        let l = |v: f64| v.log10();
        let s = |v: f64| fmt_eur_l10_with(l(v), Notation::Suffixes);
        let m = |v: f64| fmt_eur_l10_with(l(v), Notation::Mots);
        assert_eq!(s(512.0), "512 €");
        assert_eq!(s(4_500.0), "4,5 K€");
        assert_eq!(s(65_536.0), "65,5 K€");
        assert_eq!(s(4.5e6), "4,5 M€");
        assert_eq!(s(4.5e9), "4,5 B€");
        assert_eq!(s(4.5e15), "4,5 Qa€");
        assert_eq!(s(999_999.0), "1 M€");
        assert_eq!(s(1e36), "1 aa€");
        assert_eq!(m(4_500.0), "4 500 €");
        assert_eq!(m(1.5e6), "1,5 million €");
        assert_eq!(m(4.5e9), "4,5 milliards €");
        assert_eq!(m(4.5e15), "4,5 billiards €");
        assert_eq!(fmt_eur_l10_with(15.653, Notation::Scientifique), "4,5 × 10^15 €");
        assert_eq!(s(0.0078125), "0,0078 €");
    }
}
