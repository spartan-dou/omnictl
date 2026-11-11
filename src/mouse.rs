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

pub const DPI_STAGES: usize = 4;
pub const DPI_STEP: u16 = 50;
pub const DPI_MIN: u16 = 100;
pub const DPI_MAX: u16 = 42_000;
/// Fréquences de scrutation, dans l'ordre de leur index.
pub const POLLING_HZ: [u16; 4] = [125, 250, 500, 1000];
/// Couleur d'origine de chaque palier DPI (celles qu'envoie Armoury Crate).
const DPI_COLORS: [Rgb; DPI_STAGES] = [Rgb(255, 0, 0), Rgb(193, 0, 255), Rgb(0, 61, 255), Rgb(49, 255, 0)];

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
    pub color: Rgb,
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
            color: Rgb(light[6], light[7], light[8]),
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

    /// DPI des quatre paliers, relus sur la souris (`12 04 02` : X puis Y de chaque palier,
    /// en index de 50 DPI ; on renvoie X). Mesuré : 400 / 800 / 1600 / 3200 d'origine.
    pub fn dpi(&mut self) -> Result<[u16; DPI_STAGES]> {
        let r = self.link.request(&[0x12, 0x04, 0x02])?;
        let mut out = [0u16; DPI_STAGES];
        for (i, slot) in out.iter_mut().enumerate() {
            let index = u16::from_le_bytes([r[4 + 4 * i], r[5 + 4 * i]]);
            *slot = index.saturating_add(1).saturating_mul(DPI_STEP);
        }
        Ok(out)
    }

    /// Règle le DPI d'un palier (0 à 3) : `51 31 <palier> 00 <index u16> R V B`.
    /// La souris ne vérifie aucune borne : c'est fait ici. La couleur du palier (celle que
    /// la LED montre au changement de palier) est renvoyée avec, comme le fait ASUS.
    pub fn set_dpi(&mut self, stage: usize, dpi: u16) -> Result<()> {
        if stage >= DPI_STAGES {
            return Err(Error::Msg("palier DPI : 1 à 4".into()));
        }
        if !(DPI_MIN..=DPI_MAX).contains(&dpi) || dpi % DPI_STEP != 0 {
            return Err(Error::Msg(format!("DPI : {DPI_MIN} à {DPI_MAX}, par pas de {DPI_STEP}")));
        }
        let index = (dpi / DPI_STEP - 1).to_le_bytes();
        let c = DPI_COLORS[stage];
        self.link
            .request(&[0x51, 0x31, stage as u8, 0x00, index[0], index[1], c.0, c.1, c.2])
            .map(|_| ())
    }

    /// Palier DPI actif (0 à 3) et index de fréquence de scrutation (voir [`POLLING_HZ`]).
    pub fn performance(&mut self) -> Result<(usize, usize)> {
        let stage = self.link.request(&[0x12, 0x00])?[11];
        let rate = self.link.request(&[0x12, 0x04])?[12] & 0x07;
        // Le palier est numéroté à partir de 1 dans cette réponse.
        Ok((stage.saturating_sub(1) as usize, rate as usize))
    }

    /// Fréquence de scrutation : index dans [`POLLING_HZ`]. Les fréquences supérieures à
    /// 1000 Hz demandent un accessoire qu'ASUS vérifie avant de les proposer ; on s'y tient.
    pub fn set_polling(&mut self, index: usize) -> Result<()> {
        if index >= POLLING_HZ.len() {
            return Err(Error::Msg("fréquence : 125, 250, 500 ou 1000 Hz".into()));
        }
        self.link.request(&[0x51, 0x31, 0x04, 0x00, index as u8, 0x00]).map(|_| ())
    }

    /// Enregistre les réglages courants dans le profil actif de la souris.
    pub fn save(&mut self) -> Result<()> {
        self.link.request(&[0x50, 0x03]).map(|_| ())
    }
}
