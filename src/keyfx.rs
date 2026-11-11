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
}

impl KeyFx {
    pub const ALL: [(KeyFx, &'static str, &'static str); 4] = [
        (KeyFx::Fixed, "Fixe", ""),
        (KeyFx::Breathe, "Respiration", "b"),
        (KeyFx::Cycle, "Cycle", "c"),
        (KeyFx::Wave, "Vague", "w"),
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
    pub fn at(&self, t: f32, x: f32) -> Rgb {
        let pulse = |phase: f32| 0.12 + 0.88 * (0.5 - 0.5 * (phase * std::f32::consts::TAU).cos());
        match self.fx {
            KeyFx::Fixed => self.color,
            KeyFx::Breathe => scale(self.color, pulse(t / 3.0)),
            KeyFx::Wave => scale(self.color, pulse(t / 2.4 - x / 78.0)),
            KeyFx::Cycle => {
                let (h, s, v) = rgb_to_hsv(self.color);
                // Une couleur grise n'a pas de teinte à faire tourner : on lui en donne une.
                let s = if s < 0.05 { 1.0 } else { s };
                hsv_to_rgb(h + t * 60.0, s, v.max(0.2))
            }
        }
    }
}

/// Durée de l'éclair à la frappe, en secondes.
pub const FLASH_SECONDS: f32 = 0.45;

/// Superpose l'éclair de frappe : `boost` vaut 1 à l'appui puis retombe à 0.
pub fn flash(base: Rgb, boost: f32) -> Rgb {
    if boost <= 0.0 {
        return base;
    }
    let t = 0.7 * boost.min(1.0);
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
        assert_eq!(s.at(0.0, 0.0), Rgb(10, 200, 30));
        assert_eq!(s.at(123.4, 50.0), Rgb(10, 200, 30));
    }

    #[test]
    fn breathing_stays_within_the_colour_and_actually_moves() {
        let s = KeyStyle { color: Rgb(200, 100, 0), fx: KeyFx::Breathe };
        let samples: Vec<Rgb> = (0..60).map(|i| s.at(i as f32 * 0.05, 0.0)).collect();
        assert!(samples.iter().all(|c| c.0 <= 200 && c.1 <= 100 && c.2 == 0));
        let (min, max) = (samples.iter().map(|c| c.0).min().unwrap(), samples.iter().map(|c| c.0).max().unwrap());
        assert!(max >= 195 && min <= 40, "amplitude trop faible : {min}..{max}");
    }

    #[test]
    fn wave_is_breathing_shifted_by_position() {
        let s = KeyStyle { color: Rgb(255, 255, 255), fx: KeyFx::Wave };
        assert_ne!(s.at(1.0, 0.0), s.at(1.0, 39.0));
        // Même phase une période plus loin dans l'espace.
        assert_eq!(s.at(1.0, 0.0), s.at(1.0, 78.0));
    }

    #[test]
    fn cycle_keeps_brightness_and_returns_after_six_seconds() {
        let s = KeyStyle { color: Rgb(255, 0, 0), fx: KeyFx::Cycle };
        assert_eq!(s.at(0.0, 0.0), Rgb(255, 0, 0));
        assert_eq!(s.at(2.0, 0.0), Rgb(0, 255, 0));
        assert_eq!(s.at(6.0, 0.0), Rgb(255, 0, 0));
    }

    #[test]
    fn flash_brightens_then_vanishes() {
        let base = Rgb(100, 0, 0);
        assert_eq!(flash(base, 0.0), base);
        let hit = flash(base, 1.0);
        assert!(hit.0 > 200 && hit.1 > 150, "{hit:?}");
        assert!(flash(base, 0.5).1 < hit.1);
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
