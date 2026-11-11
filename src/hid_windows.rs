//! Couche HID minimale pour Windows, en FFI directe (aucune dépendance externe).
//!
//! Juste ce qu'il faut : énumérer les collections HID, en ouvrir une, écrire un rapport
//! de sortie et lire les rapports d'entrée avec échéance (E/S « overlapped »).

#![allow(non_snake_case, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::fmt;
use std::ptr::{null, null_mut};

type HANDLE = *mut c_void;
type BOOL = i32;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const DIGCF_PRESENT: u32 = 0x02;
const DIGCF_DEVICEINTERFACE: u32 = 0x10;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 1;
const FILE_SHARE_WRITE: u32 = 2;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const ERROR_IO_PENDING: u32 = 997;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

#[repr(C)]
#[derive(Clone, Copy)]
struct GUID {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[repr(C)]
struct SP_DEVICE_INTERFACE_DATA {
    cbSize: u32,
    InterfaceClassGuid: GUID,
    Flags: u32,
    Reserved: usize,
}

#[repr(C)]
struct HIDD_ATTRIBUTES {
    Size: u32,
    VendorID: u16,
    ProductID: u16,
    VersionNumber: u16,
}

/// HIDP_CAPS : 32 mots de 16 bits ; seuls les cinq premiers nous intéressent.
#[repr(C)]
struct HIDP_CAPS {
    Usage: u16,
    UsagePage: u16,
    InputReportByteLength: u16,
    OutputReportByteLength: u16,
    FeatureReportByteLength: u16,
    _rest: [u16; 27],
}

#[repr(C)]
struct OVERLAPPED {
    Internal: usize,
    InternalHigh: usize,
    Offset: u32,
    OffsetHigh: u32,
    hEvent: HANDLE,
}

#[link(name = "setupapi")]
extern "system" {
    fn SetupDiGetClassDevsW(guid: *const GUID, enumerator: *const u16, hwnd: HANDLE, flags: u32) -> HANDLE;
    fn SetupDiEnumDeviceInterfaces(
        set: HANDLE,
        devinfo: *const c_void,
        guid: *const GUID,
        index: u32,
        data: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> BOOL;
    fn SetupDiGetDeviceInterfaceDetailW(
        set: HANDLE,
        data: *const SP_DEVICE_INTERFACE_DATA,
        detail: *mut c_void,
        size: u32,
        required: *mut u32,
        devinfo: *mut c_void,
    ) -> BOOL;
    fn SetupDiDestroyDeviceInfoList(set: HANDLE) -> BOOL;
}

#[link(name = "hid")]
extern "system" {
    fn HidD_GetHidGuid(guid: *mut GUID);
    fn HidD_GetAttributes(h: HANDLE, attrs: *mut HIDD_ATTRIBUTES) -> u8;
    fn HidD_GetPreparsedData(h: HANDLE, data: *mut *mut c_void) -> u8;
    fn HidD_FreePreparsedData(data: *mut c_void) -> u8;
    fn HidP_GetCaps(data: *mut c_void, caps: *mut HIDP_CAPS) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        disposition: u32,
        flags: u32,
        template: HANDLE,
    ) -> HANDLE;
    fn CloseHandle(h: HANDLE) -> BOOL;
    fn ReadFile(h: HANDLE, buf: *mut u8, len: u32, read: *mut u32, ov: *mut OVERLAPPED) -> BOOL;
    fn WriteFile(h: HANDLE, buf: *const u8, len: u32, written: *mut u32, ov: *mut OVERLAPPED) -> BOOL;
    fn CreateEventW(security: *const c_void, manual: BOOL, initial: BOOL, name: *const u16) -> HANDLE;
    fn ResetEvent(h: HANDLE) -> BOOL;
    fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    fn GetOverlappedResult(h: HANDLE, ov: *mut OVERLAPPED, transferred: *mut u32, wait: BOOL) -> BOOL;
    fn CancelIoEx(h: HANDLE, ov: *mut OVERLAPPED) -> BOOL;
    fn GetLastError() -> u32;
}

#[derive(Debug)]
pub struct HidError(pub &'static str, pub u32);

impl fmt::Display for HidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (code Windows {})", self.0, self.1)
    }
}

fn last(what: &'static str) -> HidError {
    HidError(what, unsafe { GetLastError() })
}

/// Une collection HID de premier niveau présente sur le système.
#[derive(Clone, Debug)]
pub struct Info {
    pub path: Vec<u16>,
    pub pid: u16,
    pub usage_page: u16,
    pub input_len: usize,
    pub output_len: usize,
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Énumère les collections HID dont le VID correspond.
pub fn enumerate(vid: u16) -> Vec<Info> {
    let mut out = Vec::new();
    unsafe {
        let mut guid = GUID { d1: 0, d2: 0, d3: 0, d4: [0; 8] };
        HidD_GetHidGuid(&mut guid);
        let set = SetupDiGetClassDevsW(&guid, null(), null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if set == INVALID_HANDLE_VALUE {
            return out;
        }
        for index in 0.. {
            let mut data = SP_DEVICE_INTERFACE_DATA {
                cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                InterfaceClassGuid: guid,
                Flags: 0,
                Reserved: 0,
            };
            if SetupDiEnumDeviceInterfaces(set, null(), &guid, index, &mut data) == 0 {
                break;
            }
            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &data, null_mut(), 0, &mut required, null_mut());
            if required < 6 {
                continue;
            }
            // SP_DEVICE_INTERFACE_DETAIL_DATA_W : cbSize (u32) suivi du chemin UTF-16.
            // Tampon de u32 pour garantir l'alignement ; cbSize vaut 8 sur les cibles 64 bits.
            let mut detail = vec![0u32; (required as usize).div_ceil(4) + 1];
            detail[0] = 8;
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &data,
                detail.as_mut_ptr() as *mut c_void,
                required,
                null_mut(),
                null_mut(),
            ) == 0
            {
                continue;
            }
            let chars = (required as usize - 4) / 2;
            let p = (detail.as_ptr() as *const u8).add(4) as *const u16;
            let mut path: Vec<u16> = (0..chars).map(|i| p.add(i).read_unaligned()).collect();
            if let Some(end) = path.iter().position(|&c| c == 0) {
                path.truncate(end);
            }
            path.push(0);

            // Accès 0 : suffit pour interroger les attributs, même sur les collections
            // clavier/souris que Windows ouvre en exclusivité.
            let h = Handle(CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            ));
            if h.0 == INVALID_HANDLE_VALUE {
                continue;
            }
            let mut attrs = HIDD_ATTRIBUTES {
                Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32,
                VendorID: 0,
                ProductID: 0,
                VersionNumber: 0,
            };
            if HidD_GetAttributes(h.0, &mut attrs) == 0 || attrs.VendorID != vid {
                continue;
            }
            let mut pp: *mut c_void = null_mut();
            if HidD_GetPreparsedData(h.0, &mut pp) == 0 {
                continue;
            }
            let mut caps: HIDP_CAPS = std::mem::zeroed();
            let status = HidP_GetCaps(pp, &mut caps);
            HidD_FreePreparsedData(pp);
            if status != HIDP_STATUS_SUCCESS {
                continue;
            }
            out.push(Info {
                path,
                pid: attrs.ProductID,
                usage_page: caps.UsagePage,
                input_len: caps.InputReportByteLength as usize,
                output_len: caps.OutputReportByteLength as usize,
            });
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

/// Une collection HID ouverte en lecture/écriture.
pub struct Device {
    h: Handle,
    read_event: Handle,
    write_event: Handle,
    read_ov: Box<OVERLAPPED>,
    read_buf: Box<[u8]>,
    read_pending: bool,
    output_len: usize,
}

fn new_event() -> Result<Handle, HidError> {
    let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
    if ev.is_null() {
        return Err(last("CreateEvent"));
    }
    Ok(Handle(ev))
}

impl Device {
    pub fn open(info: &Info) -> Result<Device, HidError> {
        let h = unsafe {
            CreateFileW(
                info.path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(last("ouverture du périphérique HID"));
        }
        let h = Handle(h);
        let read_event = new_event()?;
        let write_event = new_event()?;
        let read_ov = Box::new(OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Offset: 0,
            OffsetHigh: 0,
            hEvent: read_event.0,
        });
        Ok(Device {
            h,
            read_event,
            write_event,
            read_ov,
            read_buf: vec![0u8; info.input_len.max(1)].into_boxed_slice(),
            read_pending: false,
            output_len: info.output_len,
        })
    }

    /// Écrit un rapport de sortie. `report[0]` est le report ID (0 si le périphérique n'en
    /// utilise pas) ; le rapport est complété par des zéros à la longueur attendue.
    pub fn write(&mut self, report: &[u8]) -> Result<(), HidError> {
        let mut buf = vec![0u8; self.output_len.max(report.len())];
        buf[..report.len()].copy_from_slice(report);
        let mut ov = OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Offset: 0,
            OffsetHigh: 0,
            hEvent: self.write_event.0,
        };
        unsafe {
            ResetEvent(self.write_event.0);
            if WriteFile(self.h.0, buf.as_ptr(), buf.len() as u32, null_mut(), &mut ov) == 0 {
                if GetLastError() != ERROR_IO_PENDING {
                    return Err(last("écriture HID"));
                }
                if WaitForSingleObject(self.write_event.0, 1000) != WAIT_OBJECT_0 {
                    CancelIoEx(self.h.0, &mut ov);
                    // Attendre la fin de l'annulation : `ov` vit sur la pile.
                    let mut n = 0u32;
                    GetOverlappedResult(self.h.0, &mut ov, &mut n, 1);
                    return Err(HidError("écriture HID : délai dépassé", 0));
                }
            }
            let mut n = 0u32;
            if GetOverlappedResult(self.h.0, &mut ov, &mut n, 1) == 0 {
                return Err(last("écriture HID"));
            }
        }
        Ok(())
    }

    /// Lit un rapport d'entrée (report ID compris en tête). `None` si rien avant l'échéance.
    /// Une lecture en attente est conservée d'un appel à l'autre : aucun rapport n'est perdu.
    pub fn read(&mut self, timeout_ms: u32) -> Result<Option<&[u8]>, HidError> {
        unsafe {
            if !self.read_pending {
                ResetEvent(self.read_event.0);
                let ok = ReadFile(
                    self.h.0,
                    self.read_buf.as_mut_ptr(),
                    self.read_buf.len() as u32,
                    null_mut(),
                    &mut *self.read_ov,
                );
                if ok == 0 && GetLastError() != ERROR_IO_PENDING {
                    return Err(last("lecture HID"));
                }
                self.read_pending = true;
            }
            match WaitForSingleObject(self.read_event.0, timeout_ms) {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Ok(None),
                _ => return Err(last("attente de lecture HID")),
            }
            let mut n = 0u32;
            self.read_pending = false;
            if GetOverlappedResult(self.h.0, &mut *self.read_ov, &mut n, 1) == 0 {
                return Err(last("lecture HID"));
            }
            Ok(Some(&self.read_buf[..n as usize]))
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if self.read_pending {
            unsafe {
                CancelIoEx(self.h.0, &mut *self.read_ov);
                let mut n = 0u32;
                // Attendre la fin de l'annulation avant de libérer le tampon et l'OVERLAPPED.
                GetOverlappedResult(self.h.0, &mut *self.read_ov, &mut n, 1);
            }
        }
    }
}
