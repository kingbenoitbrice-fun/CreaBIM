//! The interface around the views: menus, tools, the project browser and
//! the status bar are web pages (the webview Tauri uses, through wry),
//! placed as child windows over the egui window. The 3D and 2D views stay
//! native underneath.
//!
//! Pages talk to the app with JSON: `window.ipc.postMessage` one way and
//! `CB.state(...)` scripts the other.

use eframe::egui;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebView, WebViewBuilder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Top,
    Left,
    Status,
    Popup,
    /// The 3D view's navigation buttons, over its top-right corner.
    Nav,
    /// The guide, at the right of the window while it is open.
    Help,
}

const PANES: usize = 6;

impl Pane {
    /// In the order they are made, which stacks the popup on top.
    const ALL: [Pane; PANES] = [
        Pane::Top,
        Pane::Left,
        Pane::Status,
        Pane::Nav,
        Pane::Help,
        Pane::Popup,
    ];

    fn name(self) -> &'static str {
        match self {
            Pane::Top => "top",
            Pane::Left => "left",
            Pane::Status => "status",
            Pane::Popup => "popup",
            Pane::Nav => "nav",
            Pane::Help => "help",
        }
    }
}

/// A message from a page, with the pane it came from.
pub struct Msg {
    pub pane: Pane,
    pub body: Value,
}

pub struct WebUi {
    views: Vec<(Pane, WebView)>,
    inbox: Arc<Mutex<Vec<Msg>>>,
    /// The last state sent to each pane, so unchanged state isn't re-sent.
    sent: [String; PANES],
    /// Where each pane is, in points.
    pub rects: [egui::Rect; PANES],
    popup_shown: bool,
    #[cfg(target_os = "linux")]
    x11: Option<X11Focus>,
}

const CSS: &str = include_str!("../ui/app.css");
const JS: &str = include_str!("../ui/app.js");
const ICONS: &str = include_str!("../ui/icons.js");

fn page(pane: Pane) -> String {
    // the guide's pages are in its pane from the start, for searching
    let pages = match pane {
        Pane::Help => format!(
            "<script>window.HELP = {};</script>",
            crate::help::pages_json().to_string().replace("</", "<\\/")
        ),
        _ => String::new(),
    };
    format!(
        "<!doctype html><html data-pane=\"{}\"><head><meta charset=\"utf-8\"><style>{CSS}</style></head>\
         <body><div id=\"root\"></div><script>{ICONS}</script>{pages}<script>{JS}</script></body></html>",
        pane.name()
    )
}

impl WebUi {
    /// Make the pages as children of the app window. Fails where child
    /// webviews aren't possible (Wayland without XWayland), and the app then
    /// keeps its egui panels.
    pub fn new(frame: &eframe::Frame) -> Result<Self, String> {
        let inbox: Arc<Mutex<Vec<Msg>>> = Default::default();
        let mut views = vec![];
        for pane in Pane::ALL {
            let inbox = inbox.clone();
            let view = WebViewBuilder::new()
                .with_html(page(pane))
                .with_bounds(Rect {
                    position: PhysicalPosition::new(0, 0).into(),
                    size: PhysicalSize::new(10, 10).into(),
                })
                .with_background_color(BACKGROUND)
                .with_ipc_handler(move |r| {
                    if let Ok(body) = serde_json::from_str(r.body()) {
                        inbox.lock().unwrap().push(Msg { pane, body });
                    }
                })
                .build_as_child(frame)
                .map_err(|e| e.to_string())?;
            views.push((pane, view));
        }
        Ok(Self {
            views,
            inbox,
            sent: Default::default(),
            rects: [egui::Rect::NOTHING; PANES],
            popup_shown: false,
            #[cfg(target_os = "linux")]
            x11: X11Focus::new(frame),
        })
    }

    fn view(&self, pane: Pane) -> &WebView {
        &self.views.iter().find(|(p, _)| *p == pane).unwrap().1
    }

    /// Let the pages run: on Linux they live on GTK, which needs pumping.
    pub fn pump() {
        #[cfg(target_os = "linux")]
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn take(&self) -> Vec<Msg> {
        std::mem::take(&mut *self.inbox.lock().unwrap())
    }

    /// Put a pane over `rect` (in points).
    pub fn place(&mut self, pane: Pane, rect: egui::Rect, ppp: f32) {
        let i = pane as usize;
        if self.rects[i] == rect {
            return;
        }
        self.rects[i] = rect;
        let _ = self.view(pane).set_bounds(Rect {
            position: PhysicalPosition::new(
                (rect.min.x * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
            )
            .into(),
            size: PhysicalSize::new(
                (rect.width() * ppp).round().max(1.0) as u32,
                (rect.height() * ppp).round().max(1.0) as u32,
            )
            .into(),
        });
    }

    /// Send a pane its state, if it changed since last time.
    pub fn send(&mut self, pane: Pane, state: &Value) {
        let text = state.to_string();
        let i = pane as usize;
        if self.sent[i] == text {
            return;
        }
        let _ = self
            .view(pane)
            .evaluate_script(&format!("window.CB && CB.state({text})"));
        self.sent[i] = text;
    }

    /// Run a script in a pane's page.
    pub fn run(&self, pane: Pane, script: &str) {
        let _ = self.view(pane).evaluate_script(script);
    }

    /// Re-send every state next frame (after a page reloads).
    pub fn resend(&mut self, pane: Pane) {
        self.sent[pane as usize].clear();
    }

    pub fn show_popup(&mut self, rect: egui::Rect, ppp: f32) {
        self.place(Pane::Popup, rect, ppp);
        self.popup_shown = true;
    }

    /// The popup is parked off the window rather than hidden: GTK child
    /// windows don't always hide and show again cleanly.
    pub fn hide_popup(&mut self) {
        if self.popup_shown || self.rects[Pane::Popup as usize] == egui::Rect::NOTHING {
            self.place(Pane::Popup, PARKED, 1.0);
            self.popup_shown = false;
            let _ = self
                .view(Pane::Popup)
                .evaluate_script("window.CB && CB.state({items: []})");
            self.sent[Pane::Popup as usize].clear();
        }
    }

    /// Hide the panes for the classic panels, or let them come back.
    /// Like the popup, they are parked off the window rather than hidden.
    pub fn hide_all(&mut self, hidden: bool) {
        for pane in Pane::ALL {
            if hidden {
                self.place(pane, PARKED, 1.0);
            } else {
                // placed again on the next frame
                self.rects[pane as usize] = egui::Rect::NOTHING;
            }
        }
        self.popup_shown = false;
    }

    /// Park a pane off the window, as for the popup.
    pub fn park(&mut self, pane: Pane) {
        self.place(pane, PARKED, 1.0);
    }

    /// Give a pane the keyboard, for typing in it straight away.
    pub fn focus(&mut self, pane: Pane) {
        let _ = self.view(pane).focus();
    }

    /// Hand the keyboard back to the views after a click on a page.
    pub fn focus_views(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(x) = &self.x11 {
            x.focus();
            return;
        }
        let _ = self.view(Pane::Top).focus_parent();
    }
}

const PARKED: egui::Rect = egui::Rect {
    min: egui::pos2(-4000.0, -4000.0),
    max: egui::pos2(-3990.0, -3990.0),
};

/// The page background, so a page never flashes white while loading.
const BACKGROUND: (u8, u8, u8, u8) = (23, 25, 30, 255);

/// Giving the keyboard back to the app window. GTK's own call only reaches
/// the page's container, so this asks the X server directly.
#[cfg(target_os = "linux")]
struct X11Focus {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    window: std::ffi::c_ulong,
}

#[cfg(target_os = "linux")]
impl X11Focus {
    fn new(frame: &eframe::Frame) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let window = match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(w) => w.window,
            _ => return None,
        };
        let xlib = x11_dl::xlib::Xlib::open().ok()?;
        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return None;
        }
        Some(Self {
            xlib,
            display,
            window,
        })
    }

    fn focus(&self) {
        unsafe {
            (self.xlib.XSetInputFocus)(
                self.display,
                self.window,
                x11_dl::xlib::RevertToParent,
                x11_dl::xlib::CurrentTime,
            );
            (self.xlib.XFlush)(self.display);
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for X11Focus {
    fn drop(&mut self) {
        unsafe { (self.xlib.XCloseDisplay)(self.display) };
    }
}
