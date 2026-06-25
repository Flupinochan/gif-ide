use crate::{AppWindow, MessageDialog};
use slint::{ComponentHandle, SharedString};

// window/dialog表示用macro
// windowとdialogで型は異なるが使用方法は同じためmacroで定義
// 1. set theme
// 2. centralize window
// 3. focus window
macro_rules! show_window {
    ($window:expr, $parent:expr) => {{
        $window
            .global::<crate::Palette>()
            .set_color_scheme($parent.global::<crate::Palette>().get_color_scheme());

        let parent_pos = $parent.window().position();
        let parent_size = $parent.window().size();
        $window.window().set_position(slint::PhysicalPosition::new(
            parent_pos.x + parent_size.width as i32 / 2,
            parent_pos.y + parent_size.height as i32 / 2,
        ));

        $window.show().unwrap();
        crate::window::focus_window($window.window());
    }};
}
pub(crate) use show_window;

pub(crate) fn show_message_dialog(title: &str, message: &str, parent: &AppWindow) {
    let dialog = MessageDialog::new().unwrap();
    dialog.set_title_text(SharedString::from(title));
    dialog.set_message(SharedString::from(message));
    let dialog_weak = dialog.as_weak();
    dialog.on_close(move || {
        if let Some(d) = dialog_weak.upgrade() {
            d.hide().unwrap();
        }
    });
    show_window!(dialog, parent);
}

// すでにwindowが表示中の場合は2重表示せず最前面に表示
#[cfg(windows)]
pub(crate) fn focus_window(window: &slint::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let slint_handle = window.window_handle();
    let Ok(handle) = slint_handle.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        let hwnd = handle.hwnd.get() as *mut std::ffi::c_void;
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn focus_window(_window: &slint::Window) {}
