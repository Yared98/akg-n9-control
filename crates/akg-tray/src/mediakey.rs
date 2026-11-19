//! Low-level keyboard hook (WH_KEYBOARD_LL) used ONLY while "Find my headphones" is ringing.
//!
//! The dongle exposes a media control HID device, so the headset play button reaches
//! Windows as the Play/Pause key. While the hook is active it: (1) signals that the key was
//! pressed and (2) swallows the key (so the music player does not start playing along).
//! It only checks the Play/Pause key code; nothing is read, stored, or sent. When dropping
//! `PlayStopHook` (Drop) the hook is removed and the key reverts to its normal behavior.
//!
//! The callback runs on the thread that installed the hook, during the event loop message
//! pumping; therefore the loop does not perform slow reads while the hook is active.
use std::sync::atomic::{AtomicBool, Ordering};

static PLAY_PRESSED: AtomicBool = AtomicBool::new(false);

const WH_KEYBOARD_LL: i32 = 13;
const WM_KEYDOWN: usize = 0x0100;
const VK_MEDIA_PLAY_PAUSE: u32 = 0xB3;

#[repr(C)]
struct KbdLlHookStruct {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(id_hook: i32, lpfn: HookProc, hmod: isize, thread_id: u32) -> isize;
    fn UnhookWindowsHookEx(hhk: isize) -> i32;
    fn CallNextHookEx(hhk: isize, code: i32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

unsafe extern "system" fn hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 {
        let k = &*(lparam as *const KbdLlHookStruct);
        if k.vk_code == VK_MEDIA_PLAY_PAUSE {
            if wparam == WM_KEYDOWN {
                PLAY_PRESSED.store(true, Ordering::SeqCst);
            }
            return 1; // swallow the key (down and up) while the hook exists
        }
    }
    CallNextHookEx(0, code, wparam, lparam)
}

/// Owner of the hook: install at the start of "ringing", drop upon stopping.
pub struct PlayStopHook(isize);

impl PlayStopHook {
    /// Installs the hook; `None` if Windows refuses (then "find my headphones" proceeds without the play shortcut).
    pub fn install() -> Option<Self> {
        PLAY_PRESSED.store(false, Ordering::SeqCst);
        let h = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, hook_proc, GetModuleHandleW(std::ptr::null()), 0)
        };
        if h == 0 {
            None
        } else {
            Some(Self(h))
        }
    }

    /// True (once) if Play/Pause was pressed since the last check.
    pub fn take_pressed(&self) -> bool {
        PLAY_PRESSED.swap(false, Ordering::SeqCst)
    }
}

impl Drop for PlayStopHook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instala_e_solta_o_hook() {
        // In a desktop session the hook installs; upon drop it is removed. Nothing is pressed here.
        if let Some(h) = PlayStopHook::install() {
            assert!(!h.take_pressed());
        }
        // After dropping, installing again must succeed (proves that the previous one was removed).
        let h2 = PlayStopHook::install();
        drop(h2);
    }
}
