//! Effets touche par touche, calculés par l'ordinateur.
//!
//! Le clavier ne sait animer que des effets globaux. Ici, chaque touche a sa couleur et son
//! propre effet ; le programme calcule une image une trentaine de fois par seconde et
//! l'envoie en mode direct. Ces effets n'existent donc que tant que le programme tourne.

use crate::proto::Rgb;

/// Effet d'une touche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyFx {
    /// Couleur fixe.
    #[default]
    Fixed,
    /// La couleur pulse lentement.
    Breathe,
    /// La teinte fait le tour du cercle des couleurs, en partant de la couleur choisie.
    Cycle,
    /// Respiration décalée selon la position : une vague traverse le clavier.
    Wave,
    /// La touche prend sa couleur dans le dégradé commun, selon sa position.
    Gradient,
}

impl KeyFx {
    pub const ALL: [(KeyFx, &'static str, &'static str); 5] = [
        (KeyFx::Fixed, "Fixe", ""),
        (KeyFx::Breathe, "Respiration", "b"),
        (KeyFx::Cycle, "Cycle", "c"),
        (KeyFx::Wave, "Vague", "w"),
        (KeyFx::Gradient, "Dégradé", "g"),
    ];

    /// Code d'une lettre utilisé dans le fichier de configuration.
    pub fn code(self) -> &'static str {
        KeyFx::ALL.iter().find(|(fx, _, _)| *fx == self).map_or("", |(_, _, code)| code)
    }

    pub fn from_code(code: &str) -> Option<KeyFx> {
        KeyFx::ALL.iter().find(|(_, _, c)| *c == code).map(|(fx, _, _)| *fx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct KeyStyle {
    pub color: Rgb,
    pub fx: KeyFx,
}

impl KeyStyle {
    pub fn fixed(color: Rgb) -> KeyStyle {
        KeyStyle { color, fx: KeyFx::Fixed }
    }

    /// Couleur de la touche à l'instant `t` (secondes). `x` est sa position horizontale,
    /// en quarts de touche (0 à gauche, environ 78 à droite).
    pub fn at(&self, t: f32, x: f32, gradient: &Gradient) -> Rgb {
        let pulse = |phase: f32| 0.12 + 0.88 * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos());
        match self.fx {
            KeyFx::Fixed => self.color,
            KeyFx::Breathe => scale(self.color, pulse(t / 3.0)),
            KeyFx::Wave => scale(self.color, pulse(t / 2.4 - x / 78.0)),
            KeyFx::Gradient => gradient.at(x / 78.0, t),
            KeyFx::Cycle => {
                let (h, s, v) = rgb_to_hsv(self.color);
                // Une couleur grise n'a pas de teinte à faire tourner : on lui en donne une.
                let s = if s < 0.05 { 1.0 } else { s };
                hsv_to_rgb(h + t * 60.0, s, v.max(0.2))
            }
        }
    }
}

/// Dégradé commun à toutes les touches réglées sur [`KeyFx::Gradient`].
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// Couleurs, de gauche à droite.
    pub stops: Vec<Rgb>,
    /// Le dégradé défile vers la droite au lieu de rester en place.
    pub scroll: bool,
}

impl Default for Gradient {
    fn default() -> Gradient {
        Gradient { stops: vec![Rgb(255, 0, 64), Rgb(128, 0, 255), Rgb(0, 64, 255)], scroll: false }
    }
}

impl Gradient {
    /// Durée d'un tour complet du dégradé qui défile, en secondes.
    const PERIOD: f32 = 6.0;

    /// Couleur à la position `pos` (0 = bord gauche, 1 = bord droit) à l'instant `t`.
    pub fn at(&self, pos: f32, t: f32) -> Rgb {
        let n = self.stops.len();
        if n == 0 {
            return Rgb::BLACK;
        }
        if n == 1 {
            return self.stops[0];
        }
        // Fixe : la première couleur à gauche, la dernière à droite. Défilant : une boucle
        // fermée (la dernière couleur rejoint la première), sans cassure au passage.
        let (segments, phase) = if self.scroll {
            (n, (pos - t / Self::PERIOD).rem_euclid(1.0))
        } else {
            (n - 1, pos.clamp(0.0, 1.0))
        };
        let scaled = phase * segments as f32;
        let i = (scaled as usize).min(segments - 1);
        let (a, b) = (self.stops[i], self.stops[(i + 1) % n]);
        let k = scaled - i as f32;
        let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
        Rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
    }
}

/// Vitesse de l'onde de frappe, en quarts de touche par seconde, et sa durée de vie.
pub const RIPPLE_SPEED: f32 = 90.0;
pub const RIPPLE_SECONDS: f32 = 0.5;

/// Intensité (0 à 1) de l'onde de frappe sur une touche située à `dist` quarts de touche
/// du point d'appui, `age` secondes après l'appui : un anneau qui s'élargit en s'éteignant.
pub fn ripple(age: f32, dist: f32) -> f32 {
    if !(0.0..RIPPLE_SECONDS).contains(&age) {
        return 0.0;
    }
    const WIDTH: f32 = 7.0;
    let ring = 1.0 - ((dist - age * RIPPLE_SPEED).abs() / WIDTH).min(1.0);
    ring * (1.0 - age / RIPPLE_SECONDS)
}

/// Durée de l'éclair à la frappe, en secondes.
pub const FLASH_SECONDS: f32 = 0.25;

/// Superpose l'éclair de frappe : `boost` vaut 1 à l'appui puis retombe à 0.
/// Blanc franc pendant le premier tiers, puis retour rapide à la couleur de la touche.
pub fn flash(base: Rgb, boost: f32) -> Rgb {
    if boost <= 0.0 {
        return base;
    }
    let t = (boost * 1.5).min(1.0);
    let f = |c: u8| (c as f32 + (255.0 - c as f32) * t).round() as u8;
    Rgb(f(base.0), f(base.1), f(base.2))
}

fn scale(c: Rgb, k: f32) -> Rgb {
    let f = |v: u8| (v as f32 * k).round().clamp(0.0, 255.0) as u8;
    Rgb(f(c.0), f(c.1), f(c.2))
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(to(r), to(g), to(b))
}

pub fn rgb_to_hsv(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_never_changes() {
        let s = KeyStyle::fixed(Rgb(10, 200, 30));
        let g = Gradient::default();
        assert_eq!(s.at(0.0, 0.0, &g), Rgb(10, 200, 30));
        assert_eq!(s.at(123.4, 50.0, &g), Rgb(10, 200, 30));
    }

    #[test]
    fn breathing_stays_within_the_colour_and_actually_moves() {
        let s = KeyStyle { color: Rgb(200, 100, 0), fx: KeyFx::Breathe };
        let g = Gradient::default();
        let samples: Vec<Rgb> = (0..60).map(|i| s.at(i as f32 * 0.05, 0.0, &g)).collect();
        assert!(samples.iter().all(|c| c.0 <= 200 && c.1 <= 100 && c.2 == 0));
        let (min, max) = (samples.iter().map(|c| c.0).min().unwrap(), samples.iter().map(|c| c.0).max().unwrap());
        assert!(max >= 195 && min <= 40, "amplitude trop faible : {min}..{max}");
    }

    #[test]
    fn wave_is_breathing_shifted_by_position() {
        let s = KeyStyle { color: Rgb(255, 255, 255), fx: KeyFx::Wave };
        let g = Gradient::default();
        assert_ne!(s.at(1.0, 0.0, &g), s.at(1.0, 39.0, &g));
        // Même phase une période plus loin dans l'espace.
        assert_eq!(s.at(1.0, 0.0, &g), s.at(1.0, 78.0, &g));
    }

    #[test]
    fn cycle_keeps_brightness_and_returns_after_six_seconds() {
        let s = KeyStyle { color: Rgb(255, 0, 0), fx: KeyFx::Cycle };
        let g = Gradient::default();
        assert_eq!(s.at(0.0, 0.0, &g), Rgb(255, 0, 0));
        assert_eq!(s.at(2.0, 0.0, &g), Rgb(0, 255, 0));
        assert_eq!(s.at(6.0, 0.0, &g), Rgb(255, 0, 0));
    }

    #[test]
    fn flash_brightens_then_vanishes() {
        let base = Rgb(100, 0, 0);
        assert_eq!(flash(base, 0.0), base);
        assert_eq!(flash(base, 1.0), Rgb(255, 255, 255));
        assert_eq!(flash(base, 0.8), Rgb(255, 255, 255), "blanc tenu au début de l'éclair");
        let fading = flash(base, 0.3);
        assert!(fading.1 > 0 && fading.1 < 255, "{fading:?}");
    }

    #[test]
    fn fixed_gradient_runs_from_first_to_last_colour() {
        let g = Gradient { stops: vec![Rgb(255, 0, 0), Rgb(0, 255, 0), Rgb(0, 0, 255)], scroll: false };
        assert_eq!(g.at(0.0, 0.0), Rgb(255, 0, 0));
        assert_eq!(g.at(0.5, 0.0), Rgb(0, 255, 0));
        assert_eq!(g.at(1.0, 0.0), Rgb(0, 0, 255));
        assert_eq!(g.at(0.25, 0.0), Rgb(128, 128, 0));
        assert_eq!(g.at(0.5, 99.0), Rgb(0, 255, 0), "un dégradé fixe ne dépend pas du temps");
    }

    #[test]
    fn scrolling_gradient_loops_without_a_seam() {
        let g = Gradient { stops: vec![Rgb(255, 0, 0), Rgb(0, 0, 255)], scroll: true };
        // Boucle fermée : le bord droit rejoint le bord gauche.
        assert_eq!(g.at(0.0, 0.0), g.at(1.0, 0.0));
        // Un tour complet ramène au point de départ, et il bouge entre-temps.
        assert_eq!(g.at(0.3, 0.0), g.at(0.3, 6.0));
        assert_ne!(g.at(0.3, 0.0), g.at(0.3, 1.5));
    }

    #[test]
    fn degenerate_gradients() {
        assert_eq!(Gradient { stops: vec![], scroll: true }.at(0.4, 1.0), Rgb::BLACK);
        assert_eq!(Gradient { stops: vec![Rgb(1, 2, 3)], scroll: true }.at(0.4, 1.0), Rgb(1, 2, 3));
    }

    #[test]
    fn ripple_is_a_ring_that_travels_and_dies() {
        assert_eq!(ripple(0.0, 0.0), 1.0, "plein sur la touche pressée, à l'instant de l'appui");
        assert_eq!(ripple(0.0, 30.0), 0.0, "pas encore arrivée au loin");
        assert!(ripple(0.2, 18.0) > 0.5, "l'anneau est passé à 18 quarts de touche après 0,2 s");
        assert_eq!(ripple(0.2, 0.0), 0.0, "le centre est déjà retombé");
        assert_eq!(ripple(RIPPLE_SECONDS, 45.0), 0.0, "éteinte en fin de vie");
        assert_eq!(ripple(-0.1, 0.0), 0.0);
    }

    #[test]
    fn hsv_round_trip() {
        for c in [Rgb(255, 0, 0), Rgb(12, 200, 99), Rgb(0, 0, 255), Rgb(40, 40, 40), Rgb(255, 255, 255)] {
            let (h, s, v) = rgb_to_hsv(c);
            assert_eq!(hsv_to_rgb(h, s, v), c);
        }
    }

    #[test]
    fn codes_round_trip() {
        for (fx, _, _) in KeyFx::ALL {
            assert_eq!(KeyFx::from_code(fx.code()), Some(fx));
        }
        assert_eq!(KeyFx::from_code("zz"), None);
    }
}
