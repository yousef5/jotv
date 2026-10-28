//! A native child window inside the app window that MPV renders into (`--wid`),
//! so live TV plays in the app instead of a separate window.
//!
//! Linux needs X11: MPV can't embed into Wayland surfaces, so on Wayland sessions
//! the app runs through XWayland (see `run()` in lib.rs). Windows uses a child
//! HWND. Elsewhere these commands return an error and the frontend falls back
//! to MPV's own window.

use tauri::AppHandle;

#[cfg(target_os = "linux")]
mod imp {
    use gtk::prelude::*;
    use std::cell::RefCell;
    use std::sync::mpsc;
    use tauri::{AppHandle, Manager};

    thread_local! {
        // GTK objects aren't Send; they live on the GTK main thread only.
        static SURFACE: RefCell<Option<gdk::Window>> = const { RefCell::new(None) };
    }

    /// Runs `f` on the GTK main thread with the main webview and waits for its result.
    fn with_webview<T: Send + 'static>(
        app: &AppHandle,
        f: impl FnOnce(webkit2gtk::WebView) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let window = app
            .get_webview_window("main")
            .ok_or("Main window not found")?;
        let (tx, rx) = mpsc::channel();
        window
            .with_webview(move |wv| {
                tx.send(f(wv.inner())).ok();
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    /// Webview origin inside its toplevel window, plus that toplevel's GdkWindow.
    fn toplevel(webview: &webkit2gtk::WebView) -> Result<(gdk::Window, i32, i32), String> {
        let top = webview.toplevel().ok_or("Webview has no toplevel")?;
        let gdk_top = top.window().ok_or("Toplevel not realized")?;
        let (x, y) = webview
            .translate_coordinates(&top, 0, 0)
            .ok_or("Cannot locate webview in window")?;
        Ok((gdk_top, x, y))
    }

    pub fn attach(app: &AppHandle) -> Result<u64, String> {
        with_webview(app, |webview| {
            if let Some(xid) = SURFACE.with(|s| s.borrow().as_ref().map(xid_of)) {
                return xid;
            }
            let (parent, _, _) = toplevel(&webview)?;
            if parent.downcast_ref::<gdkx11::X11Window>().is_none() {
                return Err("In-app video needs X11 (running on Wayland)".into());
            }
            let attrs = gdk::WindowAttr {
                window_type: gdk::WindowType::Child,
                wclass: gdk::WindowWindowClass::InputOutput,
                x: Some(0),
                y: Some(0),
                width: 1,
                height: 1,
                ..Default::default()
            };
            let surface = gdk::Window::new(Some(&parent), &attrs);
            if !surface.ensure_native() {
                surface.destroy();
                return Err("Could not create a native video surface".into());
            }
            let xid = xid_of(&surface);
            SURFACE.with(|s| *s.borrow_mut() = Some(surface));
            xid
        })
    }

    fn xid_of(window: &gdk::Window) -> Result<u64, String> {
        window
            .downcast_ref::<gdkx11::X11Window>()
            .map(|w| w.xid() as u64)
            .ok_or_else(|| "Video surface is not an X11 window".to_string())
    }

    pub fn place(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, visible: bool) -> Result<(), String> {
        with_webview(app, move |webview| {
            let (_, ox, oy) = toplevel(&webview)?;
            SURFACE.with(|s| {
                let s = s.borrow();
                let surface = s.as_ref().ok_or("No video surface")?;
                if visible && w >= 1.0 && h >= 1.0 {
                    surface.move_resize(
                        ox + x.round() as i32,
                        oy + y.round() as i32,
                        w.round() as i32,
                        h.round() as i32,
                    );
                    surface.show();
                    surface.raise();
                } else {
                    surface.hide();
                }
                Ok(())
            })
        })
    }

    pub fn detach(app: &AppHandle) -> Result<(), String> {
        with_webview(app, |_| {
            SURFACE.with(|s| {
                if let Some(surface) = s.borrow_mut().take() {
                    surface.destroy();
                }
            });
            Ok(())
        })
    }
}

#[cfg(windows)]
mod imp {
    //! A plain black child window of the main window. It sits above WebView2's own
    //! child window, so MPV's picture shows over the page at the player's spot.

    use std::sync::{mpsc, Mutex};
    use tauri::{AppHandle, Manager};
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowPos, HWND_TOP,
        SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WNDCLASSW, WS_CHILD,
        WS_CLIPSIBLINGS,
    };

    // HWNDs aren't Send; keep the raw value. Only touched on the main thread.
    static SURFACE: Mutex<Option<isize>> = Mutex::new(None);

    /// Runs `f` on the main (UI) thread and waits for its result: a window belongs
    /// to the thread that creates it, and only the main thread pumps messages.
    fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        let handle = app.clone();
        app.run_on_main_thread(move || {
            tx.send(f(&handle)).ok();
        })
        .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    pub fn attach(app: &AppHandle) -> Result<u64, String> {
        on_main(app, |app| {
            if let Some(h) = *SURFACE.lock().unwrap() {
                return Ok(h as u64);
            }
            let parent = app.get_webview_window("main").ok_or("Main window not found")?.hwnd().map_err(|e| e.to_string())?;
            unsafe {
                let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                let class = WNDCLASSW {
                    lpfnWndProc: Some(wndproc),
                    hInstance: instance.into(),
                    hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                    lpszClassName: w!("JoTVVideoSurface"),
                    ..Default::default()
                };
                // Fails harmlessly once the class exists (after a detach/attach)
                RegisterClassW(&class);
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("JoTVVideoSurface"),
                    w!(""),
                    WS_CHILD | WS_CLIPSIBLINGS,
                    0,
                    0,
                    1,
                    1,
                    Some(parent),
                    None,
                    Some(instance.into()),
                    None,
                )
                .map_err(|e| format!("Could not create a video surface: {e}"))?;
                *SURFACE.lock().unwrap() = Some(hwnd.0 as isize);
                Ok(hwnd.0 as u64)
            }
        })
    }

    pub fn place(app: &AppHandle, x: f64, y: f64, w: f64, h: f64, visible: bool) -> Result<(), String> {
        on_main(app, move |app| {
            let Some(raw) = *SURFACE.lock().unwrap() else { return Err("No video surface".into()) };
            let hwnd = HWND(raw as *mut _);
            // The page works in CSS pixels; the window in physical ones
            let scale = app.get_webview_window("main").and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
            let px = |v: f64| (v * scale).round() as i32;
            unsafe {
                if visible && w >= 1.0 && h >= 1.0 {
                    SetWindowPos(hwnd, Some(HWND_TOP), px(x), px(y), px(w), px(h), SWP_NOACTIVATE | SWP_SHOWWINDOW)
                } else {
                    SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_HIDEWINDOW)
                }
                .map_err(|e| e.to_string())
            }
        })
    }

    pub fn detach(app: &AppHandle) -> Result<(), String> {
        on_main(app, |_| {
            if let Some(raw) = SURFACE.lock().unwrap().take() {
                unsafe { DestroyWindow(HWND(raw as *mut _)).ok() };
            }
            Ok(())
        })
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    use tauri::AppHandle;

    const UNSUPPORTED: &str = "In-app video isn't supported on this platform yet";

    pub fn attach(_: &AppHandle) -> Result<u64, String> { Err(UNSUPPORTED.into()) }
    pub fn place(_: &AppHandle, _: f64, _: f64, _: f64, _: f64, _: bool) -> Result<(), String> { Err(UNSUPPORTED.into()) }
    pub fn detach(_: &AppHandle) -> Result<(), String> { Ok(()) }
}

/// Creates (or returns) the in-app video surface and its native window id for `mpv --wid`.
#[tauri::command]
pub async fn embed_attach(app: AppHandle) -> Result<u64, String> {
    imp::attach(&app)
}

/// Positions the video surface, in CSS pixels relative to the webview.
#[tauri::command]
pub async fn embed_place(app: AppHandle, x: f64, y: f64, width: f64, height: f64, visible: bool) -> Result<(), String> {
    imp::place(&app, x, y, width, height, visible)
}

#[tauri::command]
pub async fn embed_detach(app: AppHandle) -> Result<(), String> {
    imp::detach(&app)
}
