//! Media playback control via Windows Runtime (WinRT) GSMTC.
//!
//! Swallows all errors and pauses any active media sessions without synthesizing
//! Play/Pause keystrokes (to ensure music is never started if paused).

#[cfg(windows)]
pub fn pause_current() -> bool {
    pause_current_inner().unwrap_or(false)
}

#[cfg(windows)]
fn pause_current_inner() -> windows::core::Result<bool> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    let mut paused = false;
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;

    // Pause all sessions from GetSessions()
    if let Ok(sessions) = manager.GetSessions() {
        for session in sessions {
            if let Ok(op) = session.TryPauseAsync() {
                if let Ok(success) = op.get() {
                    if success {
                        paused = true;
                    }
                }
            }
        }
    }

    // Also try GetCurrentSession() if available
    if let Ok(session) = manager.GetCurrentSession() {
        if let Ok(op) = session.TryPauseAsync() {
            if let Ok(success) = op.get() {
                if success {
                    paused = true;
                }
            }
        }
    }

    Ok(paused)
}

#[cfg(not(windows))]
pub fn pause_current() -> bool {
    false
}
