/// Apply on the thread that owns the native window. This changes the whole window, not CSS opacity.
pub fn apply(window: &tauri::WebviewWindow, hide_from_capture: bool, opacity_percent: u8) -> Result<(), String> {
    let alpha = opacity_alpha(opacity_percent)?;
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|error| format!("Не удалось получить окно: {error}"))?;
        native::apply(hwnd.0, hide_from_capture, alpha)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, hide_from_capture, alpha);
        Err("Защита от захвата и прозрачность окна доступны в Windows".into())
    }
}

fn opacity_alpha(percent: u8) -> Result<u8, String> {
    if !(30..=100).contains(&percent) { return Err("Непрозрачность окна должна быть от 30 до 100%".into()); }
    Ok(((u16::from(percent) * 255 + 50) / 100) as u8)
}

#[cfg(windows)]
mod native {
    use std::ffi::c_void;
    type Hwnd = *mut c_void;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const LWA_ALPHA: u32 = 2;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;

    #[link(name = "user32")]
    extern "system" {
        #[cfg_attr(target_pointer_width = "64", link_name = "GetWindowLongPtrW")]
        #[cfg_attr(target_pointer_width = "32", link_name = "GetWindowLongW")]
        fn get_window_long(hwnd: Hwnd, index: i32) -> isize;
        #[cfg_attr(target_pointer_width = "64", link_name = "SetWindowLongPtrW")]
        #[cfg_attr(target_pointer_width = "32", link_name = "SetWindowLongW")]
        fn set_window_long(hwnd: Hwnd, index: i32, value: isize) -> isize;
        fn SetWindowDisplayAffinity(hwnd: Hwnd, affinity: u32) -> i32;
        fn GetWindowDisplayAffinity(hwnd: Hwnd, affinity: *mut u32) -> i32;
        fn SetLayeredWindowAttributes(hwnd: Hwnd, color_key: u32, alpha: u8, flags: u32) -> i32;
        fn GetLayeredWindowAttributes(hwnd: Hwnd, color_key: *mut u32, alpha: *mut u8, flags: *mut u32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn SetLastError(error: u32);
        fn GetLastError() -> u32;
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "dwmapi")]
    extern "system" { fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32; }
    #[repr(C)]
    struct OsVersion { size: u32, major: u32, minor: u32, build: u32, platform: u32, service_pack: [u16; 128] }
    #[link(name = "ntdll")]
    extern "system" { fn RtlGetVersion(version: *mut OsVersion) -> i32; }

    #[derive(Clone, Copy)]
    struct Layered { color_key: u32, alpha: u8, flags: u32 }

    fn error(action: &str) -> String { format!("{action}: {}", std::io::Error::last_os_error()) }
    fn ex_style(hwnd: Hwnd) -> Result<isize, String> {
        unsafe {
            SetLastError(0);
            let style = get_window_long(hwnd, GWL_EXSTYLE);
            if style == 0 && GetLastError() != 0 { return Err(error("Не удалось прочитать стиль окна")); }
            Ok(style)
        }
    }
    fn set_style(hwnd: Hwnd, style: isize) -> Result<(), String> {
        unsafe {
            SetLastError(0);
            if set_window_long(hwnd, GWL_EXSTYLE, style) == 0 && GetLastError() != 0 {
                return Err(error("Не удалось изменить стиль окна"));
            }
        }
        Ok(())
    }
    fn layered(hwnd: Hwnd) -> Result<Layered, String> {
        let mut value = Layered { color_key: 0, alpha: 255, flags: 0 };
        if unsafe { GetLayeredWindowAttributes(hwnd, &mut value.color_key, &mut value.alpha, &mut value.flags) } == 0 {
            return Err(error("Не удалось проверить прозрачность окна"));
        }
        Ok(value)
    }
    fn set_layered(hwnd: Hwnd, value: Layered) -> Result<(), String> {
        if unsafe { SetLayeredWindowAttributes(hwnd, value.color_key, value.alpha, value.flags) } == 0 {
            return Err(error("Не удалось изменить прозрачность окна"));
        }
        Ok(())
    }
    fn affinity(hwnd: Hwnd) -> Result<u32, String> {
        let mut value = 0;
        if unsafe { GetWindowDisplayAffinity(hwnd, &mut value) } == 0 {
            return Err(error("Windows не подтвердила защиту окна от захвата"));
        }
        Ok(value)
    }
    fn set_affinity(hwnd: Hwnd, value: u32) -> Result<(), String> {
        if unsafe { SetWindowDisplayAffinity(hwnd, value) } == 0 {
            return Err(error("Не удалось изменить защиту окна от захвата"));
        }
        Ok(())
    }
    fn require_capture_support() -> Result<(), String> {
        let mut version = OsVersion { size: std::mem::size_of::<OsVersion>() as u32, major: 0, minor: 0, build: 0, platform: 0, service_pack: [0; 128] };
        if unsafe { RtlGetVersion(&mut version) } != 0 { return Err("Не удалось определить версию Windows".into()); }
        if version.major < 10 || (version.major == 10 && version.build < 19041) {
            return Err("Исключение окна из захвата требует Windows 10 версии 2004 или новее".into());
        }
        let mut enabled = 0;
        if unsafe { DwmIsCompositionEnabled(&mut enabled) } < 0 || enabled == 0 {
            return Err("Защита от захвата требует работающего оконного композитора Windows".into());
        }
        Ok(())
    }
    fn rollback(hwnd: Hwnd, style: isize, previous: Option<Layered>, previous_affinity: Option<u32>) -> Result<(), String> {
        let mut failures = Vec::new();
        if let Some(value) = previous_affinity { if let Err(error) = set_affinity(hwnd, value) { failures.push(error); } }
        if let Some(value) = previous { if let Err(error) = set_layered(hwnd, value) { failures.push(error); } }
        if let Err(error) = set_style(hwnd, style) { failures.push(error); }
        if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
    }

    pub(super) fn apply(hwnd: Hwnd, hide_from_capture: bool, alpha: u8) -> Result<(), String> {
        if unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) } != unsafe { GetCurrentThreadId() } {
            return Err("Вид окна нужно изменять в потоке, которому принадлежит окно".into());
        }
        if hide_from_capture { require_capture_support()?; }
        let old_style = ex_style(hwnd)?;
        let old_layered = if old_style & WS_EX_LAYERED != 0 { Some(layered(hwnd)?) } else { None };
        let mut old_affinity = None;
        let result = (|| {
            if old_layered.is_none() {
                set_style(hwnd, old_style | WS_EX_LAYERED)?;
                set_layered(hwnd, Layered { color_key: 0, alpha: 255, flags: LWA_ALPHA })?;
            }
            old_affinity = Some(affinity(hwnd)?);
            let desired = Layered { alpha, ..old_layered.unwrap_or(Layered { color_key: 0, alpha: 255, flags: LWA_ALPHA }) };
            let desired = Layered { flags: desired.flags | LWA_ALPHA, ..desired };
            set_layered(hwnd, desired)?;
            let desired_affinity = if hide_from_capture { WDA_EXCLUDEFROMCAPTURE } else { 0 };
            set_affinity(hwnd, desired_affinity)?;
            let actual = layered(hwnd)?;
            if actual.alpha != alpha || actual.flags != desired.flags || actual.color_key != desired.color_key
                || affinity(hwnd)? != desired_affinity || ex_style(hwnd)? != (old_style | WS_EX_LAYERED) {
                return Err("Windows не подтвердила запрошенный вид окна".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            return match rollback(hwnd, old_style, old_layered, old_affinity) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!("{error}. Не удалось полностью вернуть прежний вид окна: {rollback_error}")),
            };
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[link(name = "user32")]
        extern "system" {
            fn CreateWindowExW(ex_style: u32, class: *const u16, title: *const u16, style: u32,
                x: i32, y: i32, width: i32, height: i32, parent: Hwnd, menu: *mut c_void,
                instance: *mut c_void, parameter: *mut c_void) -> Hwnd;
            fn DestroyWindow(hwnd: Hwnd) -> i32;
        }
        struct HiddenWindow(Hwnd);
        impl Drop for HiddenWindow { fn drop(&mut self) { unsafe { DestroyWindow(self.0); } } }
        #[test]
        #[ignore = "Requires a Windows desktop compositor; creates only its own invisible test window"]
        fn native_values_round_trip_on_an_invisible_owned_window() {
            require_capture_support().unwrap();
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = "Sobesednik appearance probe\0".encode_utf16().collect();
            let window = HiddenWindow(unsafe { CreateWindowExW(0x80, class.as_ptr(), title.as_ptr(), 0x80000000,
                0, 0, 16, 16, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) });
            assert!(!window.0.is_null());
            let before = ex_style(window.0).unwrap();
            apply(window.0, true, 128).unwrap();
            assert_eq!(affinity(window.0).unwrap(), WDA_EXCLUDEFROMCAPTURE);
            assert_eq!(layered(window.0).unwrap().alpha, 128);
            assert_eq!(ex_style(window.0).unwrap() & !WS_EX_LAYERED, before & !WS_EX_LAYERED);
            apply(window.0, false, 255).unwrap();
            assert_eq!(affinity(window.0).unwrap(), 0);
            assert_eq!(layered(window.0).unwrap().alpha, 255);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opacity_percentage_rounds_to_native_alpha_without_making_window_invisible() {
        assert_eq!(opacity_alpha(30).unwrap(), 77);
        assert_eq!(opacity_alpha(50).unwrap(), 128);
        assert_eq!(opacity_alpha(70).unwrap(), 179);
        assert_eq!(opacity_alpha(100).unwrap(), 255);
    }
    #[test]
    fn invalid_opacity_is_rejected_instead_of_clamped_or_wrapped() {
        for percent in [0, 1, 29, 101, 255] { assert!(opacity_alpha(percent).is_err()); }
    }
}
