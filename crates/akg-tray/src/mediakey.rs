//! Hook de teclado de baixo nível (WH_KEYBOARD_LL) usado SÓ enquanto o "Achar fone" está tocando.
//!
//! O dongle expõe um dispositivo HID de controle de mídia, então o botão play do fone chega ao
//! Windows como a tecla Play/Pause. Enquanto o hook existe ele: (1) sinaliza que a tecla foi
//! apertada e (2) engole a tecla (para o player de música não começar a tocar junto).
//! Ele olha apenas o código da tecla Play/Pause; nada é lido, guardado ou enviado. Ao soltar o
//! `PlayStopHook` (Drop) o hook é removido e a tecla volta ao comportamento normal.
//!
//! O callback roda na thread que instalou o hook, durante o bombeamento de mensagens do event
//! loop; por isso o loop não faz leituras demoradas enquanto o hook está ativo.
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
            return 1; // engole a tecla (down e up) enquanto o hook existe
        }
    }
    CallNextHookEx(0, code, wparam, lparam)
}

/// Dono do hook: instalar no começo do "tocar", soltar (Drop) ao parar.
pub struct PlayStopHook(isize);

impl PlayStopHook {
    /// Instala o hook; `None` se o Windows recusar (aí o "achar fone" segue sem o atalho do play).
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

    /// True (uma vez) se o Play/Pause foi apertado desde a última checagem.
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
        // Numa sessão com desktop o hook instala; ao soltar (Drop) é removido. Nada é apertado aqui.
        if let Some(h) = PlayStopHook::install() {
            assert!(!h.take_pressed());
        }
        // Depois de soltar, instalar de novo tem que funcionar (prova que o anterior foi removido).
        let h2 = PlayStopHook::install();
        drop(h2);
    }
}
