//! Autostart configuration via Windows Registry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
//!
//! Uses raw `advapi32` FFI without external dependencies.

use std::path::Path;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "akg-n9-control";

/// Quotes an executable path for registration in Windows `Run` key.
pub fn quote_exe_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy())
}

#[cfg(windows)]
type HKEY = isize;
#[cfg(windows)]
const HKEY_CURRENT_USER: HKEY = -2147483647; // 0x80000001 as isize (i32 sign-extended)
#[cfg(windows)]
const KEY_READ: u32 = 0x20019;
#[cfg(windows)]
const KEY_SET_VALUE: u32 = 0x0002;
#[cfg(windows)]
const REG_SZ: u32 = 1;
#[cfg(windows)]
const ERROR_SUCCESS: i32 = 0;

#[cfg(windows)]
#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        hkey: HKEY,
        lpsubkey: *const u16,
        uloptions: u32,
        samsired: u32,
        phkresult: *mut HKEY,
    ) -> i32;

    fn RegCreateKeyExW(
        hkey: HKEY,
        lpsubkey: *const u16,
        reserved: u32,
        lpclass: *mut u16,
        dwoptions: u32,
        samsired: u32,
        lpsecurityattributes: *mut std::ffi::c_void,
        phkresult: *mut HKEY,
        lpdwdisposition: *mut u32,
    ) -> i32;

    fn RegQueryValueExW(
        hkey: HKEY,
        lpvaluename: *const u16,
        lpreserved: *mut u32,
        lptype: *mut u32,
        lpdata: *mut u8,
        lpcbdata: *mut u32,
    ) -> i32;

    fn RegSetValueExW(
        hkey: HKEY,
        lpvaluename: *const u16,
        reserved: u32,
        dwtype: u32,
        lpdata: *const u8,
        cbdata: u32,
    ) -> i32;

    fn RegDeleteValueW(hkey: HKEY, lpvaluename: *const u16) -> i32;

    fn RegCloseKey(hkey: HKEY) -> i32;
}

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Checks whether autostart is enabled in HKCU Run.
#[cfg(windows)]
pub fn is_enabled() -> bool {
    let subkey = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    let mut hkey: HKEY = 0;
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey)
            != ERROR_SUCCESS
        {
            return false;
        }
        let res = RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        RegCloseKey(hkey);
        res == ERROR_SUCCESS
    }
}

/// Sets or deletes the HKCU Run value.
#[cfg(windows)]
pub fn set_enabled(enable: bool) {
    let subkey = to_wide(RUN_KEY);
    let name = to_wide(VALUE_NAME);
    let mut hkey: HKEY = 0;
    unsafe {
        let mut disp: u32 = 0;
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null_mut(),
            0,
            KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut hkey,
            &mut disp,
        ) != ERROR_SUCCESS
        {
            return;
        }

        if enable {
            if let Ok(exe) = std::env::current_exe() {
                let data = to_wide(&quote_exe_path(&exe));
                let cb_data = (data.len() * std::mem::size_of::<u16>()) as u32;
                let _ = RegSetValueExW(
                    hkey,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    cb_data,
                );
            }
        } else {
            let _ = RegDeleteValueW(hkey, name.as_ptr());
        }

        RegCloseKey(hkey);
    }
}

#[cfg(not(windows))]
pub fn is_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_enabled(_enable: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_exe_path() {
        let p = Path::new(r"C:\Program Files\akg-n9-control\akg-tray.exe");
        assert_eq!(
            quote_exe_path(p),
            "\"C:\\Program Files\\akg-n9-control\\akg-tray.exe\""
        );
    }
}
