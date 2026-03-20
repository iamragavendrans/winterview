use crossbeam_channel::Sender;
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
            VK_H, VK_U,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG, WM_HOTKEY,
        },
    },
};

/// Events fired from the global hotkey thread to the GUI thread.
#[derive(Debug)]
pub enum HotkeyEvent {
    /// Ctrl+Alt+H — hide whatever window currently has focus.
    /// Carries (hwnd, pid) captured at the moment the key was pressed,
    /// before focus could shift to Winterview.
    HideActive { hwnd: u32, pid: u32 },

    /// Ctrl+Alt+U — restore the most recently hidden window.
    RestoreLast,
}

const ID_HIDE: i32 = 1;
const ID_RESTORE: i32 = 2;

/// Registers Ctrl+Alt+H and Ctrl+Alt+U as global hotkeys on a background
/// thread and sends events through `tx` whenever they fire.
///
/// The thread owns the hotkey registration for its lifetime. If the sender
/// is dropped the thread exits cleanly on the next message.
pub fn start(tx: Sender<HotkeyEvent>, self_pid: u32) {
    std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            // MOD_NOREPEAT prevents repeated events while the keys are held.
            let flags = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;

            let ok_hide = unsafe {
                RegisterHotKey(None, ID_HIDE, flags, VK_H.0 as u32).is_ok()
            };
            let ok_restore = unsafe {
                RegisterHotKey(None, ID_RESTORE, flags, VK_U.0 as u32).is_ok()
            };

            if !ok_hide {
                tracing::warn!("Could not register Ctrl+Alt+H — key may be in use by another app");
            }
            if !ok_restore {
                tracing::warn!("Could not register Ctrl+Alt+U — key may be in use by another app");
            }

            let mut msg = MSG::default();
            loop {
                // GetMessageW blocks until a message arrives.
                // Filtering to WM_HOTKEY..WM_HOTKEY keeps the thread asleep
                // the rest of the time.
                let result = unsafe {
                    GetMessageW(&mut msg, Some(HWND(std::ptr::null_mut())), WM_HOTKEY, WM_HOTKEY)
                };

                if result.0 <= 0 {
                    // WM_QUIT or error — exit cleanly.
                    break;
                }

                match msg.wParam.0 as i32 {
                    ID_HIDE => {
                        // Capture the foreground window NOW, before the
                        // Winterview window can steal focus.
                        let (hwnd, pid) = get_foreground();
                        if pid == 0 || pid == self_pid {
                            // Don't hide Winterview itself via hotkey.
                            continue;
                        }
                        if tx.send(HotkeyEvent::HideActive { hwnd, pid }).is_err() {
                            break; // receiver dropped, exit
                        }
                    }
                    ID_RESTORE => {
                        if tx.send(HotkeyEvent::RestoreLast).is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }

            // Clean up registrations before the thread exits.
            unsafe {
                let _ = UnregisterHotKey(None, ID_HIDE);
                let _ = UnregisterHotKey(None, ID_RESTORE);
            }
        })
        .expect("failed to spawn hotkey thread");
}

/// Returns (hwnd as u32, pid) of the current foreground window.
fn get_foreground() -> (u32, u32) {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return (0, 0);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (hwnd.0 as u32, pid)
    }
}
