//! Transport vers le clavier : câble USB direct ou dongle ROG Omni (2,4 GHz).
//!
//! Les deux chemins transportent les mêmes commandes ASUS. Seul l'enrobage change :
//! - USB filaire (PID 1AAE, usage page FF00) : report ID 0 + 64 octets ;
//! - dongle Omni (PID 1ACE, usage page FF00) : report ID 2 + 63 octets.

use crate::hid::{self, Device, HidError};
use std::fmt;
use std::time::{Duration, Instant};

pub const VID_ASUS: u16 = 0x0B05;
pub const PID_WIRED: u16 = 0x1AAE;
pub const PID_OMNI: u16 = 0x1ACE;
const USAGE_PAGE_VENDOR: u16 = 0xFF00;
const USAGE_PAGE_MOUSE: u16 = 0xFF01;
/// Report ID du canal clavier du dongle Omni.
const OMNI_REPORT_ID: u8 = 0x02;
/// Report ID du canal souris du dongle Omni.
const OMNI_MOUSE_REPORT_ID: u8 = 0x03;

/// Taille d'une commande ou d'une réponse, report ID exclu.
pub const PAYLOAD: usize = 64;
pub type Packet = [u8; PAYLOAD];

#[derive(Debug)]
pub enum Error {
    Hid(HidError),
    NotFound,
    /// Le clavier (ou le dongle à sa place) a répondu `FF AA`.
    Refused,
    Timeout,
    Msg(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Hid(e) => write!(f, "erreur HID : {e}"),
            Error::NotFound => f.write_str(
                "clavier introuvable : branchez le câble USB, ou le dongle ROG Omni avec le clavier en mode 2,4 GHz",
            ),
            Error::Refused => f.write_str(
                "commande refusée (FF AA) : clavier éteint, en veille, ou pas sur ce mode de connexion",
            ),
            Error::Timeout => f.write_str("pas de réponse du clavier"),
            Error::Msg(m) => f.write_str(m),
        }
    }
}

impl From<HidError> for Error {
    fn from(e: HidError) -> Self {
        Error::Hid(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Usb,
    Dongle,
}

impl Via {
    pub fn label(self) -> &'static str {
        match self {
            Via::Usb => "USB filaire",
            Via::Dongle => "dongle 2,4 GHz (ROG Omni)",
        }
    }
}

pub struct Link {
    dev: Device,
    pub via: Via,
    /// Report ID des rapports de cette collection (0 = aucun).
    report_id: u8,
    pub verbose: bool,
}

impl Link {
    /// Ouvre le clavier. `prefer` force un chemin ; sinon USB filaire d'abord, puis dongle.
    pub fn open(prefer: Option<Via>) -> Result<Link> {
        let all = hid::enumerate(VID_ASUS);
        let order: &[Via] = match prefer {
            Some(Via::Usb) => &[Via::Usb],
            Some(Via::Dongle) => &[Via::Dongle],
            None => &[Via::Usb, Via::Dongle],
        };
        for &via in order {
            let pid = match via {
                Via::Usb => PID_WIRED,
                Via::Dongle => PID_OMNI,
            };
            if let Some(info) = all
                .iter()
                .find(|d| d.pid == pid && d.usage_page == USAGE_PAGE_VENDOR)
            {
                return Ok(Link {
                    dev: Device::open(info)?,
                    via,
                    report_id: if via == Via::Dongle { OMNI_REPORT_ID } else { 0 },
                    verbose: false,
                });
            }
        }
        Err(Error::NotFound)
    }

    /// Ouvre le canal de la souris appairée au dongle Omni (collection FF01, report ID 3).
    pub fn open_mouse() -> Result<Link> {
        let all = hid::enumerate(VID_ASUS);
        let info = all
            .iter()
            .find(|d| d.pid == PID_OMNI && d.usage_page == USAGE_PAGE_MOUSE)
            .ok_or_else(|| Error::Msg("dongle ROG Omni introuvable".into()))?;
        Ok(Link {
            dev: Device::open(info)?,
            via: Via::Dongle,
            report_id: OMNI_MOUSE_REPORT_ID,
            verbose: false,
        })
    }

    /// Nombre d'octets de commande utilisables par paquet sur ce transport.
    pub fn room(&self) -> usize {
        if self.report_id == 0 {
            PAYLOAD
        } else {
            PAYLOAD - 1
        }
    }

    fn drain(&mut self) {
        while matches!(self.dev.read(0), Ok(Some(_))) {}
    }

    /// Envoie une commande sans attendre de réponse.
    pub fn write(&mut self, cmd: &[u8]) -> Result<()> {
        let mut buf = [0u8; PAYLOAD + 1];
        let room = self.room();
        let len = room + 1;
        buf[0] = self.report_id;
        if cmd.len() > room {
            return Err(Error::Msg(format!(
                "commande trop longue pour ce transport ({} > {room} octets)",
                cmd.len()
            )));
        }
        buf[1..1 + cmd.len()].copy_from_slice(cmd);
        if self.verbose {
            eprintln!("  -> {}", hex(&buf[1..len]));
        }
        self.dev.write(&buf[..len])?;
        Ok(())
    }

    /// Lit un rapport, report ID retiré. `None` si rien avant l'échéance.
    pub fn read(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        let verbose = self.verbose;
        let Some(raw) = self.dev.read(timeout.as_millis() as u32)? else {
            return Ok(None);
        };
        let mut out = [0u8; PAYLOAD];
        let body = raw.get(1..).unwrap_or(&[]);
        let n = body.len().min(PAYLOAD);
        out[..n].copy_from_slice(&body[..n]);
        if verbose {
            eprintln!("  <- {}", hex(&out));
        }
        Ok(Some(out))
    }

    /// Envoie une commande et attend la réponse, qui en reprend les deux premiers octets.
    /// Les rapports qui ne correspondent pas (événements, réponses destinées à un autre
    /// logiciel parlant au clavier en même temps) sont ignorés.
    pub fn request(&mut self, cmd: &[u8]) -> Result<Packet> {
        self.request_matching(cmd, 2, Duration::from_millis(1000))
    }

    /// Comme [`request`](Self::request), mais la réponse n'est reconnue que sur ses `echo`
    /// premiers octets (certaines commandes répondent avec un second octet différent).
    pub fn request_matching(&mut self, cmd: &[u8], echo: usize, timeout: Duration) -> Result<Packet> {
        let echo = echo.min(cmd.len());
        self.drain();
        self.write(cmd)?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout);
            }
            match self.read(left)? {
                Some(r) if r[0] == 0xFF && r[1] == 0xAA => return Err(Error::Refused),
                Some(r) if r[..echo] == cmd[..echo] => return Ok(r),
                Some(_) => continue,
                None => return Err(Error::Timeout),
            }
        }
    }
}

pub fn hex(b: &[u8]) -> String {
    let end = b.iter().rposition(|&x| x != 0).map_or(1, |p| p + 1);
    b[..end]
        .iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
