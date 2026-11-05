//! Single-thread-owned libei sender for a combined portal session.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_mapping_is_unique_live_and_never_guesses_pixels() {
        let region = Region {
            mapping: "monitor".into(),
            x: 1280.,
            y: 0.,
            width: 1706.,
            height: 960.,
        };
        assert_eq!(region.point(0., 0.).unwrap(), (1280., 0.));
        let (x, y) = region.point(1., 1.).unwrap();
        assert!(x < 2986. && x > 2985. && y < 960. && y > 959.);
        for (x, y) in [(f64::NAN, 0.), (0., f64::INFINITY), (-0.1, 0.), (1.1, 0.)] {
            assert!(region.point(x, y).is_err());
        }
        assert!(unique_region("monitor", &[region.clone()]).is_ok());
        assert!(unique_region("other-session", &[region.clone()]).is_err());
        assert!(unique_region("monitor", &[region.clone(), region]).is_err());
    }
    #[test]
    fn invalid_input_is_rejected_before_any_native_mutation() {
        use crate::remote_control_core::test_support::input_message;
        let mut pointer = input_message(42, "controller", Kind::Pointer);
        pointer.action = Some(Action::Down);
        pointer.x = Some(0.5);
        pointer.y = Some(0.5);
        assert!(validate_event(&pointer).is_ok());
        pointer.button = Some(9);
        assert!(validate_event(&pointer).is_err());
        pointer.button = Some(0);
        pointer.action = Some(Action::Unknown);
        assert!(validate_event(&pointer).is_err());
        pointer.action = Some(Action::Down);
        pointer.x = Some(f64::NAN);
        assert!(validate_event(&pointer).is_err());
        let mut key = input_message(42, "controller", Kind::Key);
        key.action = Some(Action::Down);
        key.code = Some("KeyA".into());
        assert!(validate_event(&key).is_ok());
        key.code = Some("LaunchUntrustedApplication".into());
        assert!(validate_event(&key).is_err());
        let mut text = input_message(42, "controller", Kind::Text);
        text.text = Some("composed café".into());
        assert!(validate_event(&text).is_ok());
        text.text = Some("bad\0text".into());
        assert!(validate_event(&text).is_err());
    }
}

use crate::remote_control_core::{
    RemoteControlAction as Action, RemoteControlMessage, RemoteControlType as Kind,
};
use std::collections::HashSet;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub(crate) struct Region {
    pub mapping: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Region {
    pub fn point(&self, x: f64, y: f64) -> Result<(f64, f64), String> {
        if !x.is_finite()
            || !y.is_finite()
            || !(0. ..=1.).contains(&x)
            || !(0. ..=1.).contains(&y)
            || self.width <= 0.
            || self.height <= 0.
        {
            return Err("Invalid desktop pointer coordinates".into());
        }
        // Regions are half-open. Keep normalized 1.0 inside the logical edge,
        // independent of physical capture resolution or monitor scale.
        Ok((
            self.x + x * (self.width - 0.001),
            self.y + y * (self.height - 0.001),
        ))
    }
}
fn unique_region(mapping: &str, regions: &[Region]) -> Result<Region, String> {
    let mut matches = regions
        .iter()
        .filter(|r| !mapping.is_empty() && r.mapping == mapping);
    let region = matches
        .next()
        .ok_or("No EIS region matches the selected monitor")?;
    if matches.next().is_some() {
        return Err("Ambiguous EIS mapping for the selected monitor".into());
    }
    region.point(0., 0.)?;
    Ok(region.clone())
}

// Opaque libei objects never leave the worker. Signatures follow installed
// libei-1.0/libei.h; the application links the sender library, not libeis.
type Obj = c_void;
#[link(name = "ei")]
unsafe extern "C" {
    fn ei_new_sender(data: *mut c_void) -> *mut Obj;
    fn ei_unref(ei: *mut Obj) -> *mut Obj;
    fn ei_setup_backend_fd(ei: *mut Obj, fd: c_int) -> c_int;
    fn ei_device_get_seat(device: *mut Obj) -> *mut Obj;
    fn ei_get_fd(ei: *mut Obj) -> c_int;
    fn ei_dispatch(ei: *mut Obj);
    fn ei_get_event(ei: *mut Obj) -> *mut Obj;
    fn ei_event_unref(event: *mut Obj) -> *mut Obj;
    fn ei_event_get_type(event: *mut Obj) -> c_int;
    fn ei_event_get_device(event: *mut Obj) -> *mut Obj;
    fn ei_event_get_seat(event: *mut Obj) -> *mut Obj;
    fn ei_seat_bind_capabilities(seat: *mut Obj, ...);
    fn ei_device_ref(device: *mut Obj) -> *mut Obj;
    fn ei_device_unref(device: *mut Obj) -> *mut Obj;
    fn ei_device_has_capability(device: *mut Obj, capability: c_int) -> bool;
    fn ei_device_get_region(device: *mut Obj, index: usize) -> *mut Obj;
    fn ei_region_get_mapping_id(region: *mut Obj) -> *const c_char;
    fn ei_region_get_x(region: *mut Obj) -> u32;
    fn ei_region_get_y(region: *mut Obj) -> u32;
    fn ei_region_get_width(region: *mut Obj) -> u32;
    fn ei_region_get_height(region: *mut Obj) -> u32;
    fn ei_device_start_emulating(device: *mut Obj, sequence: u32);
    fn ei_device_stop_emulating(device: *mut Obj);
    fn ei_device_frame(device: *mut Obj, time: u64);
    fn ei_now(ei: *mut Obj) -> u64;
    fn ei_device_pointer_motion_absolute(device: *mut Obj, x: f64, y: f64);
    fn ei_device_button_button(device: *mut Obj, button: u32, press: bool);
    fn ei_device_keyboard_key(device: *mut Obj, key: u32, press: bool);
    fn ei_device_scroll_delta(device: *mut Obj, x: f64, y: f64);
    fn ei_device_scroll_cancel(device: *mut Obj, x: bool, y: bool);
    fn ei_device_text_utf8_with_length(device: *mut Obj, text: *const c_char, length: usize);
}
const ABS: c_int = 2;
const KEY: c_int = 4;
const SCROLL: c_int = 16;
const BUTTON: c_int = 32;
const TEXT: c_int = 64;
struct Device {
    raw: *mut Obj,
    resumed: bool,
    sequence: u32,
    keys: HashSet<u32>,
    buttons: HashSet<u32>,
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            ei_device_unref(self.raw);
        }
    }
}
struct Context(*mut Obj);
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            ei_unref(self.0);
        }
    }
}
struct Input {
    message: RemoteControlMessage,
    reply: mpsc::SyncSender<Result<(), String>>,
}
pub(crate) struct Eis {
    tx: mpsc::SyncSender<Input>,
    alive: Arc<AtomicBool>,
    text_ready: Arc<AtomicBool>,
    cancel: CancellationToken,
    worker: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    error: Arc<std::sync::Mutex<Option<String>>>,
}
impl Eis {
    pub async fn open(
        fd: OwnedFd,
        mapping: String,
        cancel: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        let (tx, rx) = mpsc::sync_channel(128);
        let (ready, result) = tokio::sync::oneshot::channel();
        let alive = Arc::new(AtomicBool::new(false));
        let worker_alive = alive.clone();
        let text_ready = Arc::new(AtomicBool::new(false));
        let worker_text = text_ready.clone();
        let worker_cancel = cancel.clone();
        let error = Arc::new(std::sync::Mutex::new(None));
        let worker_error = error.clone();
        let worker = std::thread::Builder::new()
            .name("petal-eis".into())
            .spawn(move || {
                let outcome = worker(
                    fd,
                    mapping,
                    rx,
                    ready,
                    &worker_alive,
                    &worker_text,
                    &worker_cancel,
                );
                worker_alive.store(false, Ordering::SeqCst);
                if let Err(error) = outcome {
                    log::warn!("Desktop input session ended: {error}");
                    use crate::sync_ext::MutexExt;
                    *worker_error.lock_unpoisoned() = Some(error);
                }
                worker_cancel.cancel();
            })
            .map_err(|e| format!("Cannot create EIS worker: {e}"))?;
        result
            .await
            .map_err(|_| "EIS readiness worker ended".to_string())??;
        Ok(Arc::new(Self {
            tx,
            alive,
            text_ready,
            cancel,
            error,
            worker: std::sync::Mutex::new(Some(worker)),
        }))
    }
    pub fn is_live(&self) -> bool {
        self.alive.load(Ordering::SeqCst) && !self.cancel.is_cancelled()
    }
    pub fn supports_text(&self) -> bool {
        self.is_live() && self.text_ready.load(Ordering::SeqCst)
    }
    pub fn replay(&self, message: &RemoteControlMessage) -> Result<(), String> {
        if !self.is_live() {
            return Err("Desktop input session retired".into());
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.tx
            .try_send(Input {
                message: message.clone(),
                reply,
            })
            .map_err(|_| "Desktop input queue unavailable".to_string())?;
        result
            .recv_timeout(Duration::from_millis(500))
            .map_err(|_| {
                self.cancel.cancel();
                "Desktop input delivery timed out; session retired".to_string()
            })?
    }
    pub fn close(&self) {
        use crate::sync_ext::MutexExt;
        self.cancel.cancel();
        if let Some(worker) = self.worker.lock_unpoisoned().take() {
            let _ = worker.join();
        }
    }
    pub fn error(&self) -> Option<String> {
        use crate::sync_ext::MutexExt;
        self.error.lock_unpoisoned().clone()
    }
}
impl Drop for Eis {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn worker(
    fd: OwnedFd,
    mapping: String,
    rx: mpsc::Receiver<Input>,
    ready: tokio::sync::oneshot::Sender<Result<(), String>>,
    alive: &AtomicBool,
    text_ready: &AtomicBool,
    cancel: &CancellationToken,
) -> Result<(), String> {
    // All C pointers are confined to this thread, including reference release.
    unsafe {
        let context = Context(ei_new_sender(std::ptr::null_mut()));
        if context.0.is_null() {
            return Err("Cannot allocate libei sender".into());
        }
        if ei_setup_backend_fd(context.0, fd.into_raw_fd()) != 0 {
            return Err("Cannot connect libei to the portal EIS descriptor".into());
        }
        let mut devices: Vec<Device> = Vec::new();
        let mut ready = Some(ready);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut current_binding: Result<Binding, String> =
            Err("Waiting for EIS input devices".into());
        let outcome = (|| -> Result<(), String> {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let mut poll = libc::pollfd {
                    fd: ei_get_fd(context.0),
                    events: libc::POLLIN,
                    revents: 0,
                };
                if libc::poll(&mut poll, 1, 5) < 0 {
                    return Err("EIS descriptor polling failed".into());
                }
                ei_dispatch(context.0);
                let mut devices_changed = false;
                loop {
                    let event = ei_get_event(context.0);
                    if event.is_null() {
                        break;
                    }
                    let kind = ei_event_get_type(event);
                    let raw = ei_event_get_device(event);
                    let mut lost = false;
                    if matches!(kind, 5 | 6 | 7 | 8) {
                        devices_changed = true;
                    }
                    match kind {
                        2 => lost = true,
                        3 => {
                            ei_seat_bind_capabilities(
                                ei_event_get_seat(event),
                                ABS,
                                KEY,
                                SCROLL,
                                BUTTON,
                                TEXT,
                                0i32,
                            );
                        }
                        5 => {
                            if !raw.is_null() {
                                devices.push(Device {
                                    raw: ei_device_ref(raw),
                                    resumed: false,
                                    sequence: 0,
                                    keys: HashSet::new(),
                                    buttons: HashSet::new(),
                                });
                            }
                        }
                        6 | 7 => {
                            if let Some(device) = devices.iter_mut().find(|d| d.raw == raw) {
                                device.resumed = false;
                                device.keys.clear();
                                device.buttons.clear();
                                lost = alive.load(Ordering::SeqCst);
                            }
                            if kind == 6 {
                                devices.retain(|d| d.raw != raw);
                            }
                        }
                        8 => {
                            if let Some(device) = devices.iter_mut().find(|d| d.raw == raw) {
                                if let Some(sequence) = device.sequence.checked_add(1) {
                                    device.sequence = sequence;
                                    device.resumed = true;
                                    ei_device_start_emulating(raw, sequence);
                                } else {
                                    lost = true;
                                }
                            }
                        }
                        _ => {}
                    }
                    ei_event_unref(event);
                    if lost {
                        alive.store(false, Ordering::SeqCst);
                        return Err("EIS permission or input device ended".into());
                    }
                }
                if devices_changed {
                    current_binding = binding(&devices, &mapping);
                }
                if let Ok(binding) = &current_binding {
                    alive.store(true, Ordering::SeqCst);
                    text_ready.store(binding.text.is_some(), Ordering::SeqCst);
                    if let Some(ready) = ready.take() {
                        let _ = ready.send(Ok(()));
                    }
                } else if alive.swap(false, Ordering::SeqCst) {
                    return Err("EIS monitor mapping became unavailable".into());
                } else if Instant::now() >= deadline {
                    return Err(current_binding
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "EIS readiness timed out".into()));
                }
                while let Ok(input) = rx.try_recv() {
                    // Recheck cancellation immediately before every side effect.
                    let result = if cancel.is_cancelled()
                        || !crate::remote_control::is_authorized_input(&input.message)
                        || crate::linux_desktop_control::validate_request(&input.message).is_err()
                    {
                        Err("Desktop input retired or no longer authorized".into())
                    } else if let Ok(binding) = &current_binding {
                        inject(context.0, &mut devices, binding, &input.message)
                    } else {
                        Err("Desktop mapping is not ready".into())
                    };
                    let _ = input.reply.send(result);
                }
            }
            Ok(())
        })();
        alive.store(false, Ordering::SeqCst);
        if let Some(ready) = ready.take() {
            let _ = ready.send(Err(outcome
                .as_ref()
                .err()
                .cloned()
                .unwrap_or_else(|| "Desktop preparation cancelled".into())));
        }
        // Paused/removed devices have already been neutralized by EIS. Release
        // live devices only; never wait for an acknowledgement from a dead one.
        for device in &mut devices {
            if device.resumed {
                for key in device.keys.drain() {
                    ei_device_keyboard_key(device.raw, key, false);
                }
                for button in device.buttons.drain() {
                    ei_device_button_button(device.raw, button, false);
                }
                if ei_device_has_capability(device.raw, SCROLL) {
                    ei_device_scroll_cancel(device.raw, true, true);
                }
                ei_device_frame(device.raw, ei_now(context.0));
                ei_device_stop_emulating(device.raw);
            }
        }
        ei_dispatch(context.0);
        outcome
    }
}
struct Binding {
    pointer: usize,
    keyboard: usize,
    button: usize,
    scroll: usize,
    text: Option<usize>,
    region: Region,
}
unsafe fn binding(devices: &[Device], mapping: &str) -> Result<Binding, String> {
    let mut regions = Vec::new();
    let mut owners = Vec::new();
    for (index, device) in devices.iter().enumerate().filter(|(_, d)| d.resumed) {
        if !ei_device_has_capability(device.raw, ABS) {
            continue;
        }
        for i in 0..4096 {
            let raw = ei_device_get_region(device.raw, i);
            if raw.is_null() {
                break;
            }
            let id = ei_region_get_mapping_id(raw);
            if id.is_null() {
                continue;
            }
            if CStr::from_ptr(id).to_bytes() == mapping.as_bytes() {
                regions.push(Region {
                    mapping: mapping.to_string(),
                    x: ei_region_get_x(raw) as f64,
                    y: ei_region_get_y(raw) as f64,
                    width: ei_region_get_width(raw) as f64,
                    height: ei_region_get_height(raw) as f64,
                });
                owners.push(index);
            }
        }
    }
    let region = unique_region(mapping, &regions)?;
    let pointer = owners[0];
    let seat = ei_device_get_seat(devices[pointer].raw);
    let capability = |cap| {
        devices.iter().position(|d| {
            d.resumed && ei_device_get_seat(d.raw) == seat && ei_device_has_capability(d.raw, cap)
        })
    };
    Ok(Binding {
        pointer,
        region,
        keyboard: capability(KEY).ok_or("EIS keyboard permission is not ready")?,
        button: capability(BUTTON).ok_or("EIS pointer button permission is not ready")?,
        scroll: capability(SCROLL).ok_or("EIS scroll permission is not ready")?,
        text: capability(TEXT),
    })
}
unsafe fn inject(
    context: *mut Obj,
    devices: &mut [Device],
    binding: &Binding,
    m: &RemoteControlMessage,
) -> Result<(), String> {
    validate_event(m)?;
    let region = &binding.region;
    let index = match m.message_type {
        Kind::Key => binding.keyboard,
        Kind::Text => binding
            .text
            .ok_or("EIS compositor does not support composed text")?,
        Kind::Wheel => binding.scroll,
        _ => binding.button,
    };
    if m.message_type == Kind::Pointer {
        let (x, y) = region.point(
            m.x.ok_or("Missing pointer x")?,
            m.y.ok_or("Missing pointer y")?,
        )?;
        let pointer = devices[binding.pointer].raw;
        ei_device_pointer_motion_absolute(pointer, x, y);
        ei_device_frame(pointer, ei_now(context));
        if m.action == Some(Action::Move) {
            ei_dispatch(context);
            return Ok(());
        }
    }
    let device = &mut devices[index];
    match m.message_type {
        Kind::Pointer => {
            let button = match m.button.unwrap_or(0) {
                0 => 272,
                1 => 274,
                2 => 273,
                _ => return Err("Unsupported pointer button".into()),
            };
            match m.action {
                Some(Action::Down) => {
                    ei_device_button_button(device.raw, button, true);
                    device.buttons.insert(button);
                }
                Some(Action::Up) => {
                    ei_device_button_button(device.raw, button, false);
                    device.buttons.remove(&button);
                }
                Some(Action::Click) => {
                    ei_device_button_button(device.raw, button, true);
                    ei_device_frame(device.raw, ei_now(context));
                    ei_device_button_button(device.raw, button, false);
                }
                Some(Action::Move) => {}
                _ => return Err("Invalid pointer action".into()),
            }
        }
        Kind::Key => {
            let key = evdev_code(m.code.as_deref().ok_or("Missing physical keyboard code")?)
                .ok_or("Unsupported physical keyboard code")?;
            let press = match m.action {
                Some(Action::Down) => true,
                Some(Action::Up) => false,
                _ => return Err("Invalid key action".into()),
            };
            ei_device_keyboard_key(device.raw, key, press);
            if press {
                device.keys.insert(key);
            } else {
                device.keys.remove(&key);
            }
        }
        Kind::Wheel => {
            let x = m.delta_x.unwrap_or(0.);
            let y = m.delta_y.unwrap_or(0.);
            if !x.is_finite() || !y.is_finite() || x.abs() > 10000. || y.abs() > 10000. {
                return Err("Invalid scroll delta".into());
            }
            let multiplier = match m.delta_mode.unwrap_or(0) {
                0 => 1.,
                1 => 16.,
                2 => region.height,
                _ => return Err("Invalid scroll units".into()),
            };
            ei_device_scroll_delta(device.raw, x * multiplier, y * multiplier);
        }
        Kind::Text => {
            let text = m.text.as_deref().ok_or("Missing composed text")?;
            let text = CString::new(text).map_err(|_| "Composed text contains NUL")?;
            ei_device_text_utf8_with_length(device.raw, text.as_ptr(), text.as_bytes().len());
        }
        _ => return Err("Unsupported desktop input event".into()),
    }
    ei_device_frame(device.raw, ei_now(context));
    ei_dispatch(context);
    Ok(())
}

fn validate_event(m: &RemoteControlMessage) -> Result<(), String> {
    match m.message_type {
        Kind::Pointer => {
            let x = m.x.ok_or("Missing pointer x")?;
            let y = m.y.ok_or("Missing pointer y")?;
            if !x.is_finite()
                || !y.is_finite()
                || !(0. ..=1.).contains(&x)
                || !(0. ..=1.).contains(&y)
                || !matches!(m.button.unwrap_or(0), 0..=2)
                || !matches!(
                    m.action,
                    Some(Action::Move | Action::Down | Action::Up | Action::Click)
                )
            {
                return Err("Invalid desktop pointer event".into());
            }
        }
        Kind::Key => {
            if evdev_code(m.code.as_deref().unwrap_or("")).is_none()
                || !matches!(m.action, Some(Action::Down | Action::Up))
            {
                return Err("Invalid physical keyboard event".into());
            }
        }
        Kind::Text => {
            let text = m.text.as_deref().ok_or("Missing composed text")?;
            if text.is_empty() || text.len() > 1024 * 1024 || text.as_bytes().contains(&0) {
                return Err("Invalid composed text".into());
            }
        }
        Kind::Wheel => {
            if !m.delta_x.unwrap_or(0.).is_finite()
                || !m.delta_y.unwrap_or(0.).is_finite()
                || m.delta_x.unwrap_or(0.).abs() > 10000.
                || m.delta_y.unwrap_or(0.).abs() > 10000.
                || m.delta_mode.unwrap_or(0) > 2
            {
                return Err("Invalid desktop scroll event".into());
            }
        }
        _ => return Err("Unsupported desktop input event".into()),
    }
    Ok(())
}
fn evdev_code(code: &str) -> Option<u32> {
    Some(match code {
        "Escape" => 1,
        "Digit1" => 2,
        "Digit2" => 3,
        "Digit3" => 4,
        "Digit4" => 5,
        "Digit5" => 6,
        "Digit6" => 7,
        "Digit7" => 8,
        "Digit8" => 9,
        "Digit9" => 10,
        "Digit0" => 11,
        "Minus" => 12,
        "Equal" => 13,
        "Backspace" => 14,
        "Tab" => 15,
        "KeyQ" => 16,
        "KeyW" => 17,
        "KeyE" => 18,
        "KeyR" => 19,
        "KeyT" => 20,
        "KeyY" => 21,
        "KeyU" => 22,
        "KeyI" => 23,
        "KeyO" => 24,
        "KeyP" => 25,
        "BracketLeft" => 26,
        "BracketRight" => 27,
        "Enter" => 28,
        "ControlLeft" => 29,
        "KeyA" => 30,
        "KeyS" => 31,
        "KeyD" => 32,
        "KeyF" => 33,
        "KeyG" => 34,
        "KeyH" => 35,
        "KeyJ" => 36,
        "KeyK" => 37,
        "KeyL" => 38,
        "Semicolon" => 39,
        "Quote" => 40,
        "Backquote" => 41,
        "ShiftLeft" => 42,
        "Backslash" => 43,
        "KeyZ" => 44,
        "KeyX" => 45,
        "KeyC" => 46,
        "KeyV" => 47,
        "KeyB" => 48,
        "KeyN" => 49,
        "KeyM" => 50,
        "Comma" => 51,
        "Period" => 52,
        "Slash" => 53,
        "ShiftRight" => 54,
        "NumpadMultiply" => 55,
        "AltLeft" => 56,
        "Space" => 57,
        "CapsLock" => 58,
        "F1" => 59,
        "F2" => 60,
        "F3" => 61,
        "F4" => 62,
        "F5" => 63,
        "F6" => 64,
        "F7" => 65,
        "F8" => 66,
        "F9" => 67,
        "F10" => 68,
        "NumLock" => 69,
        "ScrollLock" => 70,
        "Numpad7" => 71,
        "Numpad8" => 72,
        "Numpad9" => 73,
        "NumpadSubtract" => 74,
        "Numpad4" => 75,
        "Numpad5" => 76,
        "Numpad6" => 77,
        "NumpadAdd" => 78,
        "Numpad1" => 79,
        "Numpad2" => 80,
        "Numpad3" => 81,
        "Numpad0" => 82,
        "NumpadDecimal" => 83,
        "IntlBackslash" => 86,
        "F11" => 87,
        "F12" => 88,
        "NumpadEnter" => 96,
        "ControlRight" => 97,
        "NumpadDivide" => 98,
        "PrintScreen" => 99,
        "AltRight" => 100,
        "Home" => 102,
        "ArrowUp" => 103,
        "PageUp" => 104,
        "ArrowLeft" => 105,
        "ArrowRight" => 106,
        "End" => 107,
        "ArrowDown" => 108,
        "PageDown" => 109,
        "Insert" => 110,
        "Delete" => 111,
        "Pause" => 119,
        "MetaLeft" => 125,
        "MetaRight" => 126,
        "ContextMenu" => 127,
        _ => return None,
    })
}
