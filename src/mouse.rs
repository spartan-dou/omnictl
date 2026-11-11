//! Souris ROG Keris II Ace (nom de code ASUS « P714 »), via le dongle ROG Omni.
//!
//! Même famille de protocole que le clavier : `12 xx` lit, `51 xx` écrit, `50 xx` gère les
//! profils. Sources : lecture du pilote ASUS des souris, OpenRGB (`AsusAuraMouseController`,
//! même disposition des octets pour d'autres souris ROG) et mesures sur le matériel.
//!
//! Seul l'éclairage est piloté pour l'instant. Les réglages de performance (DPI, fréquence
//! de scrutation) sont identifiés mais leur codage n'est pas encore établi : rien n'est écrit
//! à l'aveugle.

use crate::link::{Error, Link, Result};
use crate::proto::Rgb;

/// Effets de la LED de la souris.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseMode {
    Static = 0,
    Breathing = 1,
    ColorCycle = 2,
    /// La LED indique le niveau de batterie (réglage d'usine).
    Battery = 6,
}

impl MouseMode {
    pub const ALL: [(MouseMode, &'static str); 4] = [
        (MouseMode::Static, "Statique"),
        (MouseMode::Breathing, "Respiration"),
        (MouseMode::ColorCycle, "Cycle"),
        (MouseMode::Battery, "Batterie"),
    ];

    pub fn from_code(code: u8) -> Option<MouseMode> {
        MouseMode::ALL.iter().find(|(m, _)| *m as u8 == code).map(|(m, _)| *m)
    }

    pub fn has_color(self) -> bool {
        matches!(self, MouseMode::Static | MouseMode::Breathing)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseLight {
    pub mode: MouseMode,
    pub color: Rgb,
    /// Niveau 0 à 4 (0, 25, 50, 75, 100 %).
    pub level: u8,
}

pub struct MouseStatus {
    pub firmware: String,
    pub profile: u8,
    pub percent: u8,
    pub millivolts: u16,
    /// Effet et niveau de luminosité (0 à 4) actuellement réglés.
    pub mode: u8,
    pub level: u8,
}

pub struct Mouse {
    pub link: Link,
}

impl Mouse {
    pub fn open() -> Result<Mouse> {
        Ok(Mouse { link: Link::open_mouse()? })
    }

    pub fn status(&mut self) -> Result<MouseStatus> {
        // Via le dongle : octets 4-6 = version du récepteur, 14-16 = version de la souris.
        let v = self.link.request(&[0x12, 0x00])?;
        let power = self.link.request(&[0x12, 0x07])?;
        let light = self.link.request(&[0x12, 0x03])?;
        Ok(MouseStatus {
            firmware: format!("{:X}.{:02X}.{:02X}", v[16], v[15], v[14]),
            profile: v[10],
            percent: power[4],
            millivolts: u16::from_le_bytes([power[7], power[8]]),
            mode: light[4],
            level: light[5] / 25,
        })
    }

    /// Applique un effet à la LED : `51 28 zone 00 mode luminosité R V B`.
    /// Zone 3 = toutes les zones, celle qu'utilise le pilote ASUS pour cette souris.
    pub fn set_light(&mut self, light: &MouseLight) -> Result<()> {
        if light.level > 4 {
            return Err(Error::Msg("luminosité de la souris : niveau 0 à 4".into()));
        }
        let c = if light.mode.has_color() { light.color } else { Rgb::BLACK };
        self.link
            .request(&[0x51, 0x28, 0x03, 0x00, light.mode as u8, light.level * 25, c.0, c.1, c.2])
            .map(|_| ())
    }

    /// Enregistre les réglages courants dans le profil actif de la souris.
    pub fn save(&mut self) -> Result<()> {
        self.link.request(&[0x50, 0x03]).map(|_| ())
    }
}
