//! Native GTK camera surfaces (ADR 0003): the room-owned self-view window
//! that opens with publication, and the Settings transient preview window.
//! Both consume the same latest-frame store fed by the one capture; GTK
//! objects stay on the main thread, mirroring linux_compositor.rs.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gtk::prelude::*;

use crate::sync_ext::MutexExt;
use crate::transport::camera::CameraFrame;

/// Largest long edge a surface frame is drawn at: halving the NV12 frame
/// (2x2 box filter) before conversion keeps main-thread work bounded.
const SURFACE_MAX_LONG_EDGE: u32 = 640;
const GTK_PUMP_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Default)]
struct ViewInner {
    latest: Mutex<Option<CameraFrame>>,
    sequence: AtomicU64,
    drawn_sequence: AtomicU64,
    publication: AtomicBool,
    preview: AtomicBool,
    label: Mutex<Option<String>>,
    preview_closed: Mutex<Option<Box<dyn Fn() + Send>>>,
}

/// The one latest-frame mailbox shared by the capture pump and the GTK
/// surfaces. Publication and preview state ride along so window lifecycle
/// follows lease state, never a second capture.
#[derive(Clone)]
pub(crate) struct CameraViewStore(Arc<ViewInner>);

impl CameraViewStore {
    pub(crate) fn new() -> Self {
        Self(Arc::new(ViewInner::default()))
    }

    /// Latest-wins frame publication from the capture thread.
    pub(crate) fn publish(&self, frame: CameraFrame) {
        let mut latest = self.0.latest.lock_unpoisoned();
        *latest = Some(frame);
        self.0.sequence.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn clear(&self) {
        *self.0.latest.lock_unpoisoned() = None;
    }

    /// `label` updates the window titles only when Some.
    pub(crate) fn set_surface_state(&self, publication: bool, preview: bool, label: Option<&str>) {
        self.0.publication.store(publication, Ordering::SeqCst);
        self.0.preview.store(preview, Ordering::SeqCst);
        if let Some(label) = label {
            *self.0.label.lock_unpoisoned() = Some(label.to_string());
        }
    }

    pub(crate) fn publication_surface_active(&self) -> bool {
        self.0.publication.load(Ordering::SeqCst)
    }

    pub(crate) fn preview_surface_active(&self) -> bool {
        self.0.preview.load(Ordering::SeqCst)
    }

    /// Register the callback for the user closing the native preview
    /// window; it releases the preview lease (never the publication).
    pub(crate) fn on_preview_closed(&self, callback: Box<dyn Fn() + Send>) {
        *self.0.preview_closed.lock_unpoisoned() = Some(callback);
    }

    fn take_new_frame(&self) -> Option<CameraFrame> {
        let sequence = self.0.sequence.load(Ordering::SeqCst);
        if sequence == self.0.drawn_sequence.load(Ordering::SeqCst) {
            return None;
        }
        let frame = self.0.latest.lock_unpoisoned().take();
        if frame.is_some() {
            self.0.drawn_sequence.store(sequence, Ordering::SeqCst);
        }
        frame
    }

    fn label(&self) -> Option<String> {
        self.0.label.lock_unpoisoned().clone()
    }

    fn preview_closed(&self) {
        if let Some(callback) = self.0.preview_closed.lock_unpoisoned().as_ref() {
            callback();
        }
    }
}

/// What the GTK pump should do with the room-owned self-view this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelfViewAction {
    /// Publication is live and the user has not closed the viewer.
    Show,
    /// Publication is live but the user closed the viewer: stay hidden.
    StayHidden,
    /// Publication ended: retire the surface entirely.
    Retire,
}

/// Self-view visibility across pump ticks. Closing the window is a user
/// decision that must survive until the publication itself ends; the pump
/// runs every 16ms, so re-showing whenever the window is merely invisible
/// turned the close button into a ~150ms hide/re-map flash (observed live
/// 2026-11-06: the self-view "popped" back on every close click).
#[derive(Debug, Default)]
struct SelfViewLifecycle {
    dismissed: Arc<AtomicBool>,
}

impl SelfViewLifecycle {
    /// Shared with the window's delete handler.
    fn dismissal_flag(&self) -> Arc<AtomicBool> {
        self.dismissed.clone()
    }

    fn tick(&self, publication: bool) -> SelfViewAction {
        if !publication {
            // A later publication is a new camera session: the viewer opens
            // again, exactly like the first time.
            self.dismissed.store(false, Ordering::SeqCst);
            return SelfViewAction::Retire;
        }
        if self.dismissed.load(Ordering::SeqCst) {
            SelfViewAction::StayHidden
        } else {
            SelfViewAction::Show
        }
    }
}
struct CameraSurface {
    window: gtk::Window,
    area: gtk::DrawingArea,
    pixels: Arc<Mutex<Option<OwnedPixels>>>,
}

struct OwnedPixels {
    width: i32,
    height: i32,
    bgra: Vec<u8>,
}

impl CameraSurface {
    /// `self_view_dismissal` is Some for the room-owned self-view (closing
    /// it records a dismissal the pump honours) and None for the Settings
    /// preview (closing it releases the preview lease).
    fn new(
        title: &str,
        self_view_dismissal: Option<Arc<AtomicBool>>,
        store: &CameraViewStore,
    ) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_decorated(true);
        window.set_resizable(true);
        window.set_default_size(480, 270);
        window.set_title(title);
        window.set_type_hint(gtk::gdk::WindowTypeHint::Utility);
        let area = gtk::DrawingArea::new();
        if let Some(accessible) = area.accessible() {
            accessible.set_name(title);
        }
        let pixels: Arc<Mutex<Option<OwnedPixels>>> = Arc::new(Mutex::new(None));
        let draw_pixels = pixels.clone();
        area.connect_draw(move |widget, context| {
            context.set_source_rgb(0.04, 0.05, 0.08);
            let _ = context.paint();
            let pixels = draw_pixels.lock_unpoisoned();
            if let Some(pixels) = pixels.as_ref() {
                if let Ok(surface) = gtk::cairo::ImageSurface::create_for_data(
                    pixels.bgra.clone(),
                    gtk::cairo::Format::ARgb32,
                    pixels.width,
                    pixels.height,
                    pixels.width * 4,
                ) {
                    let width = f64::from(widget.allocated_width());
                    let height = f64::from(widget.allocated_height());
                    let scale =
                        (width / f64::from(pixels.width)).min(height / f64::from(pixels.height));
                    context.translate(
                        (width - f64::from(pixels.width) * scale) / 2.0,
                        (height - f64::from(pixels.height) * scale) / 2.0,
                    );
                    context.scale(scale, scale);
                    if context.set_source_surface(&surface, 0.0, 0.0).is_ok() {
                        let _ = context.paint();
                    }
                }
            }
            gtk::glib::Propagation::Proceed
        });
        window.add(&area);
        if let Some(dismissed) = self_view_dismissal {
            // The self-view is room-owned: closing it hides the viewer for
            // the rest of this publication but never stops the publication
            // or starts another capture.
            let hide_window = window.clone();
            window.connect_delete_event(move |_, _| {
                dismissed.store(true, Ordering::SeqCst);
                hide_window.hide();
                gtk::glib::Propagation::Stop
            });
        } else {
            let close_store = store.clone();
            window.connect_delete_event(move |_, _| {
                close_store.preview_closed();
                gtk::glib::Propagation::Proceed
            });
        }
        Self {
            window,
            area,
            pixels,
        }
    }

    fn show(&self, label: &str, title: &str) {
        let title = if label.is_empty() {
            title.to_string()
        } else {
            format!("{title} ({label})")
        };
        if self.window.title().as_deref() != Some(title.as_str()) {
            self.window.set_title(&title);
        }
        if !self.window.is_visible() {
            self.window.show_all();
        }
    }

    fn hide(&self) {
        if self.window.is_visible() {
            self.window.hide();
        }
    }

    fn update(&self, frame: &CameraFrame) {
        match convert_nv12_bgra(frame) {
            Some(pixels) => {
                *self.pixels.lock_unpoisoned() = Some(pixels);
                self.area.queue_draw();
            }
            None => log::warn!("Linux camera surface frame could not be converted"),
        }
    }
}

/// Halve an NV12 frame with a 2x2 box filter until the long edge fits the
/// surface budget (planes tightly packed, stride == width).
fn halve_nv12(frame: &CameraFrame) -> CameraFrame {
    let mut frame = frame.clone();
    while frame.width.max(frame.height) > SURFACE_MAX_LONG_EDGE
        && frame.width >= 4
        && frame.height >= 4
    {
        let width = frame.width / 2;
        let height = frame.height / 2;
        let mut y = Vec::with_capacity((width * height) as usize);
        for row in 0..height as usize {
            for column in 0..width as usize {
                let base = row * 2 * frame.y_stride as usize + column * 2;
                let a = frame.y[base] as u32;
                let b = frame.y[base + 1] as u32;
                let c = frame.y[base + frame.y_stride as usize] as u32;
                let d = frame.y[base + frame.y_stride as usize + 1] as u32;
                y.push(((a + b + c + d) / 4) as u8);
            }
        }
        let mut uv = Vec::with_capacity((width * height / 4) as usize);
        for row in 0..(height / 4) as usize {
            for column in 0..width as usize {
                let base = row * 2 * frame.uv_stride as usize + column * 2;
                let a = frame.uv[base] as u32;
                let b = frame.uv[base + 2] as u32;
                let c = frame.uv[base + frame.uv_stride as usize] as u32;
                let d = frame.uv[base + frame.uv_stride as usize + 2] as u32;
                uv.push(((a + b + c + d) / 4) as u8);
                let a = frame.uv[base + 1] as u32;
                let b = frame.uv[base + 3] as u32;
                let c = frame.uv[base + frame.uv_stride as usize + 1] as u32;
                let d = frame.uv[base + frame.uv_stride as usize + 3] as u32;
                uv.push(((a + b + c + d) / 4) as u8);
            }
        }
        frame = CameraFrame {
            width,
            height,
            y,
            y_stride: width,
            uv,
            uv_stride: width,
            capture_wall_time_us: frame.capture_wall_time_us,
        };
    }
    frame
}

/// NV12 (BT.601 limited range, the camera capture convention) to BGRA.
fn convert_nv12_bgra(frame: &CameraFrame) -> Option<OwnedPixels> {
    let frame = halve_nv12(frame);
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 || frame.y.len() < width * height {
        return None;
    }
    let mut bgra = vec![0u8; width * height * 4];
    for row in 0..height {
        for column in 0..width {
            let y = frame.y[row * frame.y_stride as usize + column] as i32;
            let uv_index = (row / 2) * frame.uv_stride as usize + (column / 2) * 2;
            let u = frame.uv[uv_index] as i32 - 128;
            let v = frame.uv[uv_index + 1] as i32 - 128;
            // BT.601 limited-range: Y [16..235] → [0..255].
            let luma = ((y - 16) * 255) / 219;
            let red = luma + (292 * v) / 256;
            let green = luma - (101 * v) / 256 - (50 * u) / 256;
            let blue = luma + (255 * u) / 256;
            let offset = (row * width + column) * 4;
            bgra[offset] = blue.clamp(0, 255) as u8;
            bgra[offset + 1] = green.clamp(0, 255) as u8;
            bgra[offset + 2] = red.clamp(0, 255) as u8;
            bgra[offset + 3] = 255;
        }
    }
    Some(OwnedPixels {
        width: width as i32,
        height: height as i32,
        bgra,
    })
}

/// The single GTK pump for every camera surface (main thread only). It is
/// started once per process by the owner; window lifecycle follows the view
/// store's lease state, never a second capture.
pub(super) fn start_camera_gtk_pump(app: tauri::AppHandle, store: CameraViewStore) {
    let mut self_view: Option<CameraSurface> = None;
    let self_view_lifecycle = SelfViewLifecycle::default();
    let mut preview: Option<CameraSurface> = None;
    // Closing the preview surface releases its lease through the owner;
    // the owner updates the store's surface state itself.
    let owner = crate::linux_camera::global_owner();
    store.on_preview_closed(Box::new(move || {
        owner.close_preview_from_surface();
    }));
    gtk::glib::timeout_add_local(GTK_PUMP_INTERVAL, move || {
        let _ = &app;
        let publication = store.publication_surface_active();
        let preview_active = store.preview_surface_active();
        let label = store.label().unwrap_or_default();
        let frame = store.take_new_frame();
        // Self-view follows the publication lease (C01/C07), except that a
        // user close keeps it hidden until that publication ends.
        match self_view_lifecycle.tick(publication) {
            SelfViewAction::Show => {
                let surface = self_view.get_or_insert_with(|| {
                    CameraSurface::new(
                        "Petal — Camera self-view",
                        Some(self_view_lifecycle.dismissal_flag()),
                        &store,
                    )
                });
                surface.show(&label, "Petal — Camera self-view");
                if let Some(frame) = &frame {
                    surface.update(frame);
                }
            }
            SelfViewAction::StayHidden => {}
            SelfViewAction::Retire => {
                if let Some(surface) = self_view.take() {
                    surface.hide();
                }
            }
        }
        // The Settings preview is explicitly visible and clearly titled
        // (C02); closing it releases the preview lease.
        if preview_active {
            let surface = preview
                .get_or_insert_with(|| CameraSurface::new("Petal — Camera preview", None, &store));
            surface.show(&label, "Petal — Camera preview");
            if let Some(frame) = &frame {
                surface.update(frame);
            }
        } else if let Some(surface) = preview.take() {
            surface.hide();
        }
        gtk::glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv12(width: u32, height: u32, fill: u8) -> CameraFrame {
        CameraFrame {
            width,
            height,
            y: vec![fill; (width * height) as usize],
            y_stride: width,
            uv: vec![128; (width * height / 2) as usize],
            uv_stride: width,
            capture_wall_time_us: 1,
        }
    }

    #[test]
    fn gray_frames_convert_to_opaque_gray_bgra() {
        let pixels = convert_nv12_bgra(&nv12(8, 8, 128)).expect("gray frame converts");
        assert_eq!((pixels.width, pixels.height), (8, 8));
        // Neutral chroma with mid luma yields an opaque gray-ish pixel whose
        // channels are equal (no color cast).
        let first = &pixels.bgra[..4];
        assert_eq!(first[0], first[1]);
        assert_eq!(first[1], first[2]);
        assert_eq!(first[3], 255);
    }

    #[test]
    fn oversized_frames_are_halved_into_the_surface_budget() {
        let downscaled = halve_nv12(&nv12(2560, 1440, 90));
        assert_eq!((downscaled.width, downscaled.height), (640, 360));
        assert_eq!(downscaled.y_stride, 640);
        assert_eq!(downscaled.y.len(), 640 * 360);
        assert_eq!(downscaled.uv.len(), 640 * 180);
        let tiny = halve_nv12(&nv12(320, 240, 90));
        assert_eq!(
            (tiny.width, tiny.height),
            (320, 240),
            "small frames are untouched"
        );
    }

    #[test]
    fn latest_frame_wins_and_drawn_frames_are_not_repeated() {
        let store = CameraViewStore::new();
        assert!(store.take_new_frame().is_none());
        store.publish(nv12(8, 8, 10));
        store.publish(nv12(8, 8, 20));
        let frame = store
            .take_new_frame()
            .expect("a published frame is available");
        assert_eq!(frame.y[0], 20, "only the newest frame survives");
        assert!(
            store.take_new_frame().is_none(),
            "drawn frames do not repeat"
        );
        store.publish(nv12(8, 8, 30));
        store.clear();
        assert!(
            store.take_new_frame().is_none(),
            "cleared store draws nothing"
        );
    }

    #[test]
    fn surface_state_flags_track_leases_and_labels_update_only_when_given() {
        let store = CameraViewStore::new();
        assert!(!store.publication_surface_active());
        assert!(!store.preview_surface_active());
        store.set_surface_state(true, true, Some("Cam-A"));
        assert!(store.publication_surface_active());
        assert!(store.preview_surface_active());
        assert_eq!(store.label().as_deref(), Some("Cam-A"));
        // Passing None keeps the current label (per-frame calls).
        store.set_surface_state(false, true, None);
        assert_eq!(store.label().as_deref(), Some("Cam-A"));
        assert!(!store.publication_surface_active());
    }

    #[test]
    fn closing_the_self_view_keeps_it_hidden_until_the_publication_ends() {
        let lifecycle = SelfViewLifecycle::default();
        assert_eq!(lifecycle.tick(true), SelfViewAction::Show);
        // The user clicks the window's close button (delete handler).
        lifecycle.dismissal_flag().store(true, Ordering::SeqCst);
        // Every later 16ms pump tick of the SAME publication must leave it
        // closed — re-showing here was the live "pops back" bug.
        for _ in 0..100 {
            assert_eq!(lifecycle.tick(true), SelfViewAction::StayHidden);
        }
        // Turning the camera off retires the viewer and clears the dismissal,
        // so the next camera session opens it again.
        assert_eq!(lifecycle.tick(false), SelfViewAction::Retire);
        assert_eq!(lifecycle.tick(true), SelfViewAction::Show);
    }
}
