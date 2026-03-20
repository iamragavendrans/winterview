use crate::hotkeys::HotkeyEvent;
use crate::native::{self, WindowInfo};
use crossbeam_channel::Receiver;
use eframe::{
    Renderer,
    egui::{
        self, Atom, AtomExt, Color32, ColorImage, FontData, FontDefinitions, FontFamily,
        FontId, IconData, Image, Margin, RichText, TextStyle, Theme, Vec2,
        ViewportCommand,
    },
};
use image::{GenericImageView, ImageFormat, ImageReader};
use std::collections::VecDeque;
use std::thread;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use std::{io::Cursor, mem};
use tracing::{debug, error, info};
use tray_icon::{
    TrayIcon, TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

struct TrayMenuIds {
    show: tray_icon::menu::MenuId,
    restore_all: tray_icon::menu::MenuId,
    quit: tray_icon::menu::MenuId,
}

enum CaptureWorkerEvent {
    Capture(Monitor),
    StopCapture,
}

#[derive(Debug)]
enum InjectorWorkerEvent {
    Update,
    PerformOp(u32, u32, bool, Option<bool>),
}

struct ScreenCapture {
    capture_send: crossbeam_channel::Sender<ColorImage>,
}

impl GraphicsCaptureApiHandler for ScreenCapture {
    type Flags = crossbeam_channel::Sender<ColorImage>;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(ScreenCapture { capture_send: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: windows_capture::graphics_capture_api::InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.capture_send.is_full() {
            return Ok(());
        }
        let width = frame.width();
        let height = frame.height();
        if let Ok(buffer) = frame.buffer() {
            let mut no_pad_buffer = Vec::new();
            let no_pad_buffer = buffer.as_nopadding_buffer(&mut no_pad_buffer);
            let img = ColorImage::from_rgba_unmultiplied(
                [width as usize, height as usize],
                &no_pad_buffer,
            );
            let _ = self.capture_send.try_send(img);
        }
        Ok(())
    }
}

struct Gui {
    monitors: Vec<Monitor>,
    windows: Arc<Mutex<Vec<WindowInfo>>>,
    event_sender: crossbeam_channel::Sender<InjectorWorkerEvent>,
    capture_event_send: crossbeam_channel::Sender<CaptureWorkerEvent>,
    capture_recv: crossbeam_channel::Receiver<ColorImage>,
    capture_tex: Option<egui::TextureHandle>,
    show_desktop_preview: bool,
    active_monitor: usize,
    icon_cache: HashMap<(u32, u32), Option<egui::TextureHandle>>,
    // --- new fields ---
    hotkey_recv: Receiver<HotkeyEvent>,
    _tray_icon: TrayIcon,
    tray_menu_ids: TrayMenuIds,
    hidden_stack: VecDeque<(u32, u32, String)>,
    window_visible: bool,
    self_pid: u32,
    /// Our own top-level HWND, found on the first update frame.
    self_hwnd: Option<u32>,
    /// Whether our window is currently excluded from screen capture.
    /// True by default — the panel is private to the local user.
    capture_excluded: bool,
    /// Keep the window floating above all other windows.
    always_on_top: bool,
}

impl Gui {
    fn new(hotkey_recv: Receiver<HotkeyEvent>) -> Gui {
        let self_pid = std::process::id();

        let windows = Arc::new(Mutex::new(Vec::new()));
        let windows_copy = windows.clone();
        let (sender, receiver) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            for event in receiver {
                match event {
                    InjectorWorkerEvent::Update => {
                        debug!("populating");
                        let mut w = native::get_top_level_windows();
                        *windows_copy.lock().unwrap() = mem::take(&mut w);
                    }
                    InjectorWorkerEvent::PerformOp(pid, hwnd, hide_window, hide_from_taskbar) => {
                        info!("op on hwnd {:?}", hwnd);
                        if let Err(e) = native::Injector::set_window_props_with_pid(
                            pid, hwnd, hide_window, hide_from_taskbar,
                        ) {
                            error!("Failed: {:?}", e);
                        }
                    }
                }
            }
        });

        let (capture_send, capture_recv) = crossbeam_channel::bounded(1);
        let (capture_event_send, capture_event_recv) = crossbeam_channel::unbounded();

        thread::spawn(move || {
            let mut active: Option<CaptureControl<_, _>> = None;
            for event in capture_event_recv.iter() {
                if let Some(ctrl) = active {
                    let _ = ctrl.stop();
                    active = None;
                }
                match event {
                    CaptureWorkerEvent::Capture(monitor) => {
                        let settings = Settings::new(
                            monitor,
                            CursorCaptureSettings::Default,
                            DrawBorderSettings::Default,
                            SecondaryWindowSettings::Default,
                            MinimumUpdateIntervalSettings::Default,
                            DirtyRegionSettings::Default,
                            ColorFormat::Rgba8,
                            capture_send.clone(),
                        );
                        if let Ok(ctrl) = ScreenCapture::start_free_threaded(settings) {
                            active = Some(ctrl);
                        }
                    }
                    CaptureWorkerEvent::StopCapture => {}
                }
            }
        });

        let monitors = Monitor::enumerate().unwrap_or_default();
        if !monitors.is_empty() {
            let _ = capture_event_send.send(CaptureWorkerEvent::Capture(monitors[0]));
        }

        // Build tray menu.
        let tray_menu = Menu::new();
        let item_show = MenuItem::new("Show Invisiwind", true, None);
        let item_restore_all = MenuItem::new("Restore all hidden windows", true, None);
        let item_quit = MenuItem::new("Quit", true, None);
        let tray_menu_ids = TrayMenuIds {
            show: item_show.id().clone(),
            restore_all: item_restore_all.id().clone(),
            quit: item_quit.id().clone(),
        };
        let _ = tray_menu.append_items(&[
            &item_show,
            &PredefinedMenuItem::separator(),
            &item_restore_all,
            &PredefinedMenuItem::separator(),
            &item_quit,
        ]);
        let tray_icon = build_tray_icon(tray_menu);

        Gui {
            show_desktop_preview: !monitors.is_empty(),
            monitors,
            windows,
            event_sender: sender,
            capture_event_send,
            capture_recv,
            capture_tex: None,
            active_monitor: 0,
            icon_cache: HashMap::new(),
            hotkey_recv,
            _tray_icon: tray_icon,
            tray_menu_ids,
            hidden_stack: VecDeque::new(),
            window_visible: true,
            self_pid,
            self_hwnd: None,
            capture_excluded: true,   // excluded from capture by default
            always_on_top: false,
        }
    }

    fn get_icon<'a>(
        icon_cache: &'a mut HashMap<(u32, u32), Option<egui::TextureHandle>>,
        ctx: &egui::Context,
        pid: u32,
        hwnd: u32,
    ) -> &'a Option<egui::TextureHandle> {
        if !icon_cache.contains_key(&(pid, hwnd)) {
            let icon = match native::get_icon(hwnd) {
                Some((width, height, buffer)) => {
                    let image = ColorImage::from_rgba_unmultiplied([width, height], &buffer);
                    Some(ctx.load_texture("icon", image, egui::TextureOptions::LINEAR))
                }
                None => None,
            };
            icon_cache.insert((pid, hwnd), icon);
        }
        icon_cache.get(&(pid, hwnd)).unwrap()
    }

    fn add_section_header(
        ui: &mut egui::Ui,
        theme: Theme,
        header: impl Into<String>,
        desc: impl Into<String>,
    ) {
        let (header_color, desc_color) = match theme {
            Theme::Light => (Color32::from_rgb(34, 34, 34), Color32::from_rgb(119, 119, 119)),
            Theme::Dark => (Color32::from_rgb(242, 242, 242), Color32::from_rgb(148, 148, 148)),
        };
        ui.label(RichText::new(header).heading().color(header_color));
        ui.label(RichText::new(desc).color(desc_color));
        ui.add_space(8.0);
    }

    fn hide_window(&mut self, pid: u32, hwnd: u32) {
        // Always remove from taskbar + Alt+Tab so it can't be seen in shared screen.
        // The window is still on screen and clickable; Ctrl+Alt+U restores it.
        if native::Injector::set_window_props_with_pid(pid, hwnd, true, Some(true)).is_ok() {
            let title = native::get_window_title(hwnd);
            self.hidden_stack.push_back((pid, hwnd, title.clone()));
            // Fix state sync gap: reflect immediately in GUI list without
            // waiting for the background thread to respond to Update.
            if let Ok(mut windows) = self.windows.try_lock() {
                if let Some(w) = windows.iter_mut().find(|w| w.hwnd == hwnd) {
                    w.hidden = true;
                } else {
                    // Window wasn't in list yet — add it so UI shows it checked.
                    windows.push(crate::native::WindowInfo {
                        hwnd,
                        pid,
                        title,
                        hidden: true,
                    });
                }
            }
            // Background refresh picks up any other changes (icons, new windows).
            self.update_tray_tooltip();
            let _ = self.event_sender.send(InjectorWorkerEvent::Update);
        } else {
            error!("hotkey hide failed for hwnd {}", hwnd);
        }
    }

    fn restore_last(&mut self) {
        if let Some((pid, hwnd, _)) = self.hidden_stack.pop_back() {
            if native::Injector::set_window_props_with_pid(pid, hwnd, false, Some(false)).is_ok() {
                // Immediate GUI sync — uncheck without waiting for background thread.
                if let Ok(mut windows) = self.windows.try_lock() {
                    if let Some(w) = windows.iter_mut().find(|w| w.hwnd == hwnd) {
                        w.hidden = false;
                    }
                }
                self.update_tray_tooltip();
                let _ = self.event_sender.send(InjectorWorkerEvent::Update);
            }
        }
    }

    fn restore_all(&mut self) {
        while let Some((pid, hwnd, _)) = self.hidden_stack.pop_back() {
            let _ = native::Injector::set_window_props_with_pid(pid, hwnd, false, Some(false));
            if let Ok(mut windows) = self.windows.try_lock() {
                if let Some(w) = windows.iter_mut().find(|w| w.hwnd == hwnd) {
                    w.hidden = false;
                }
            }
        }
        self.update_tray_tooltip();
        let _ = self.event_sender.send(InjectorWorkerEvent::Update);
    }

    fn set_window_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.window_visible = visible;
        ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
        if visible {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            if self.show_desktop_preview && !self.monitors.is_empty() {
                let _ = self.capture_event_send.send(CaptureWorkerEvent::Capture(
                    self.monitors[self.active_monitor],
                ));
            }
            let _ = self.event_sender.send(InjectorWorkerEvent::Update);
        } else {
            let _ = self.capture_event_send.send(CaptureWorkerEvent::StopCapture);
        }
    }

    fn poll_hotkeys(&mut self) {
        while let Ok(event) = self.hotkey_recv.try_recv() {
            match event {
                HotkeyEvent::HideActive { hwnd, pid } => self.hide_window(pid, hwnd),
                HotkeyEvent::RestoreLast => self.restore_last(),
            }
        }
    }

    fn update_tray_tooltip(&self) {
        let n = self.hidden_stack.len();
        let tip = if n == 0 {
            "Invisiwind — no windows hidden".to_string()
        } else {
            format!("Invisiwind — {} window{} hidden", n, if n == 1 { "" } else { "s" })
        };
        let _ = self._tray_icon.set_tooltip(Some(tip));
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event {
                let next = !self.window_visible;
                self.set_window_visible(ctx, next);
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.tray_menu_ids.show {
                self.set_window_visible(ctx, true);
            } else if event.id == self.tray_menu_ids.restore_all {
                self.restore_all();
            } else if event.id == self.tray_menu_ids.quit {
                self.restore_all();
                // Remove our own capture exclusion on exit — clean state.
                if let Some(hwnd) = self.self_hwnd {
                    native::set_self_capture_visibility(hwnd, false);
                }
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Tick even when hidden so tray clicks and hotkeys are processed.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        // Close button -> hide to tray instead.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.set_window_visible(ctx, false);
            return;
        }

        // Minimize (Win+D, taskbar button) -> hide to tray instead.
        // Without this the window becomes unreachable because with_taskbar(false)
        // leaves no way to restore a minimized window, and the ghost frame border
        // remains visible while dragging other windows over that area.
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            self.set_window_visible(ctx, false);
            return;
        }

        self.poll_hotkeys();
        self.poll_tray(ctx);

        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::WindowFocused(focused) = event {
                if focused {
                    debug!("focused");
                    let _ = self.event_sender.send(InjectorWorkerEvent::Update);
                    if self.show_desktop_preview {
                        let _ = self.capture_event_send.send(CaptureWorkerEvent::Capture(
                            self.monitors[self.active_monitor],
                        ));
                    }
                } else {
                    let _ = self.capture_event_send.send(CaptureWorkerEvent::StopCapture);
                }
            }
        }

        let theme = ctx.theme();
        let focused = ctx.input(|i| i.focused);

        // Apply always-on-top every frame so the setting takes effect immediately.
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(if self.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));

        // Every frame until found: locate our own HWND and apply capture exclusion.
        // We retry rather than giving up after one frame because EnumWindows
        // may not see our window on the very first tick.
        if self.self_hwnd.is_none() {
            if let Some(w) = native::get_top_level_windows()
                .into_iter()
                .find(|w| w.pid == self.self_pid)
            {
                self.self_hwnd = Some(w.hwnd);
                // Apply immediately — never let a single frame through unprotected.
                native::set_self_capture_visibility(w.hwnd, true);
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(Margin::same(14)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.show_desktop_preview {
                        Self::add_section_header(ui, theme, "Preview", "How others will see your screen");

                        if let Ok(img) = self.capture_recv.try_recv() {
                            if let Some(tex) = &mut self.capture_tex {
                                tex.set(img, egui::TextureOptions::LINEAR);
                            } else {
                                self.capture_tex = Some(ctx.load_texture(
                                    "screen_capture", img, egui::TextureOptions::LINEAR,
                                ));
                            }
                            ctx.request_repaint();
                        }

                        if let Some(tex) = &self.capture_tex {
                            ui.add(egui::Image::from_texture(tex).shrink_to_fit());
                        }

                        if self.monitors.len() > 1 {
                            ui.add_space(8.0);
                            ui.horizontal_wrapped(|ui| {
                                for (i, monitor) in self.monitors.iter().enumerate() {
                                    let lbl = ui.selectable_label(
                                        i == self.active_monitor, format!("Screen {}", i + 1),
                                    );
                                    if lbl.clicked() && self.active_monitor != i {
                                        self.active_monitor = i;
                                        let _ = self.capture_event_send.send(
                                            CaptureWorkerEvent::Capture(*monitor),
                                        );
                                    }
                                }
                            });
                        }

                        ui.add_space(14.0);
                    }

                    Self::add_section_header(ui, theme, "Hide applications", "Select the windows to hide");

                    let self_pid = self.self_pid;

                    for window_info in self.windows.lock().unwrap().iter_mut() {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

                        let icon_atom = if let Some(texture) = Gui::get_icon(
                            &mut self.icon_cache, ctx, window_info.pid, window_info.hwnd,
                        ) {
                            Image::from_texture(texture).max_height(16.0).atom_max_width(16.0)
                        } else {
                            Atom::grow().atom_size(Vec2::new(16.0, 0.0))
                        };

                        let checkbox_label = (
                            Atom::grow().atom_size(Vec2::new(0.0, 0.0)),
                            icon_atom,
                            Atom::grow().atom_size(Vec2::new(0.0, 0.0)),
                            &window_info.title,
                        );

                        let checkbox_response = ui.checkbox(&mut window_info.hidden, checkbox_label);

                        if checkbox_response.changed() {
                            let is_self = window_info.pid == self_pid;

                            // Always remove hidden windows from taskbar + Alt+Tab so
                            // they can't appear in screen share switch thumbnails.
                            // Invisiwind itself is exempt — it needs tray access.
                            let hide_from_taskbar = if is_self { None } else { Some(window_info.hidden) };

                            // Keep hidden_stack in sync with checkbox changes.
                            if window_info.hidden {
                                self.hidden_stack.push_back((
                                    window_info.pid,
                                    window_info.hwnd,
                                    window_info.title.clone(),
                                ));
                            } else {
                                self.hidden_stack.retain(|(_, hw, _)| *hw != window_info.hwnd);
                            }

                            let event = InjectorWorkerEvent::PerformOp(
                                window_info.pid, window_info.hwnd,
                                window_info.hidden, hide_from_taskbar,
                            );
                            self.event_sender.send(event).unwrap();
                            self.update_tray_tooltip();
                        }

                        ui.add_space(2.0);
                    }

                    ui.add_space(10.0);

                    ui.collapsing("Advanced settings", |ui| {
                        let preview_resp = ui.checkbox(
                            &mut self.show_desktop_preview, "Show desktop preview",
                        );
                        if preview_resp.changed() {
                            let event = if self.show_desktop_preview {
                                CaptureWorkerEvent::Capture(self.monitors[self.active_monitor])
                            } else {
                                self.capture_tex = None;
                                CaptureWorkerEvent::StopCapture
                            };
                            self.capture_event_send.send(event).unwrap();
                        }

                        // ── always on top ──────────────────────────────
                        ui.checkbox(&mut self.always_on_top, "Always on top");

                        // ── capture exclusion ───────────────────────────
                        let cap_resp = ui.checkbox(
                            &mut self.capture_excluded,
                            "Hide this panel from screen capture",
                        );
                        if cap_resp.changed() {
                            if let Some(hwnd) = self.self_hwnd {
                                native::set_self_capture_visibility(hwnd, self.capture_excluded);
                            }
                        }

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);

                        let hint_color = match theme {
                            Theme::Light => Color32::from_rgb(110, 110, 110),
                            Theme::Dark  => Color32::from_rgb(150, 150, 150),
                        };
                        for line in [
                            "Hotkeys (work even while browser is focused):",
                            "  Ctrl+Alt+H  — hide focused window",
                            "  Ctrl+Alt+U  — restore last hidden window",
                            "  Tray icon left-click  — show/hide this panel",
                        ] {
                            ui.label(RichText::new(line).color(hint_color).small());
                        }
                    });
                });
            });
    }
}

fn load_tray_icon_image() -> Option<tray_icon::Icon> {
    let img = ImageReader::with_format(
        Cursor::new(include_bytes!("../../Misc/invicon.ico")),
        ImageFormat::Ico,
    )
    .decode()
    .ok()?;
    let (w, h) = img.dimensions();
    let rgba = img.into_rgba8().into_raw();
    tray_icon::Icon::from_rgba(rgba, w, h).ok()
}

fn build_tray_icon(menu: Menu) -> TrayIcon {
    let mut builder = TrayIconBuilder::new()
        .with_tooltip("Invisiwind")
        .with_menu(Box::new(menu));
    if let Some(icon) = load_tray_icon_image() {
        builder = builder.with_icon(icon);
    }
    builder.build().expect("failed to create tray icon")
}

pub fn start(hotkey_recv: Receiver<HotkeyEvent>) {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 540.0])
            .with_taskbar(false),   // tray-only: no taskbar button, no Alt+Tab entry
        renderer: Renderer::Wgpu,
        ..Default::default()
    };

    if let Ok(d_image) = ImageReader::with_format(
        Cursor::new(include_bytes!("../../Misc/invicon.ico")),
        ImageFormat::Ico,
    )
    .decode()
    {
        let (width, height) = d_image.dimensions();
        options.viewport = options.viewport.with_icon(Arc::new(IconData {
            rgba: d_image.into_rgba8().into_raw(),
            width,
            height,
        }));
    }

    eframe::run_native(
        "Invisiwind",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                "Inter_18pt-Regular".to_owned(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../../Misc/fonts/Inter_18pt-Regular.ttf"
                ))),
            );
            fonts.families.insert(
                FontFamily::Name("Inter_18pt-Regular".into()),
                vec!["Inter_18pt-Regular".to_owned()],
            );
            fonts.font_data.insert(
                "Inter_18pt-Bold".to_owned(),
                Arc::new(FontData::from_static(include_bytes!(
                    "../../Misc/fonts/Inter_18pt-Bold.ttf"
                ))),
            );
            fonts.families.insert(
                FontFamily::Name("Inter_18pt-Bold".into()),
                vec!["Inter_18pt-Bold".to_owned()],
            );

            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.all_styles_mut(|style| {
                style.visuals.widgets.inactive.corner_radius = Default::default();
                style.visuals.widgets.hovered.corner_radius  = Default::default();
                style.visuals.widgets.active.corner_radius   = Default::default();
                style.visuals.widgets.hovered.bg_stroke      = Default::default();
                style.visuals.widgets.active.bg_stroke       = Default::default();
                style.visuals.widgets.hovered.expansion      = 0.0;
                style.visuals.widgets.active.expansion       = 0.0;
                style.interaction.selectable_labels          = false;

                let mut text_styles = style.text_styles.clone();
                text_styles.insert(TextStyle::Body, FontId {
                    size: 12.0,
                    family: egui::FontFamily::Name("Inter_18pt-Regular".into()),
                });
                text_styles.insert(TextStyle::Heading, FontId {
                    size: 16.0,
                    family: egui::FontFamily::Name("Inter_18pt-Bold".into()),
                });
                style.text_styles = text_styles;
            });

            Ok(Box::new(Gui::new(hotkey_recv)))
        }),
    )
    .expect("failed to create window");
}
