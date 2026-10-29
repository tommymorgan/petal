//! Camera devices and modes from the Camera portal's restricted PipeWire
//! remote (ADR 0003): registry Video/Source node discovery plus each node's
//! SPA_PARAM_EnumFormat parameters. Labels come from real node properties and
//! modes from real format parameters — never hard-coded V4L2 paths — and
//! missing usable metadata is an explicit unavailable state.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pipewire as pw;
use pipewire::registry::Registry;
use pipewire::spa;
use pipewire::spa::pod::{Pod, PodObject};
use pipewire::spa::sys;
use pipewire::spa::utils::dict::DictRef;
use pipewire::thread_loop::ThreadLoopBox;
use pipewire::{context::ContextBox, core::CoreBox, proxy::Proxy};

use crate::sync_ext::MutexExt;
use crate::transport::camera::CameraMode;

/// Waiting for the remote's answers is bounded: a portal remote that never
/// completes its initial enumeration or a node's EnumFormat reply is an
/// explicit failure, never a hang.
const REMOTE_ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_SLICE: Duration = Duration::from_millis(50);

pub(super) struct RegistryDevice {
    pub node: u32,
    pub label: String,
}

pub(super) enum MetadataEvent {
    /// The restricted remote's camera nodes changed (initial + every update).
    Devices(Vec<RegistryDevice>),
    /// The watched node disappeared from the registry.
    NodeRemoved(u32),
    /// The PipeWire core errored or disconnected: camera access is gone.
    AccessLost(String),
}

#[derive(Default)]
struct RegistryState {
    devices: BTreeMap<u32, String>,
    initial_pending: Option<i32>,
    watch: Option<u32>,
    /// Set by a core error/disconnect; never cleared without a new remote.
    lost: Option<String>,
    modes: Option<ModeRequest>,
    /// Monotonic floor for request sequence numbers (0 is reserved).
    seq_floor: i32,
}

struct ModeRequest {
    node: u32,
    seq: i32,
    done: bool,
}

/// One node EnumFormat entry: the concrete rectangles and frame rates the
/// node advertised, already collapsed from SPA choice structures.
#[derive(Default, Clone)]
struct RawFormat {
    sizes: Vec<(u32, u32)>,
    rates: Vec<(u32, u32)>,
}

pub(super) struct CameraRegistry {
    // Dropped LAST (see the explicit Drop): every PipeWire object below
    // borrows this loop, and the loop must outlive them all.
    loop_: Arc<ThreadLoopBox>,
    context: Option<ContextBox<'static>>,
    core: Option<CoreBox<'static>>,
    registry: Option<pipewire::registry::RegistryBox<'static>>,
    state: Arc<Mutex<RegistryState>>,
    events: std::sync::mpsc::Sender<MetadataEvent>,
}

// SAFETY: every PipeWire call on this registry runs while holding its
// ThreadLoop's lock, or inside a loop-thread callback; pw_thread_loop's lock
// is the documented cross-thread serialization contract for this sharing.
unsafe impl Send for CameraRegistry {}
unsafe impl Sync for CameraRegistry {}

impl CameraRegistry {
    /// Connect a dedicated registry to one restricted remote fd and start
    /// watching it. The loop thread owns all PipeWire dispatching; every
    /// other call site serializes through `lock()`.
    pub fn connect(
        remote: OwnedFd,
        events: std::sync::mpsc::Sender<MetadataEvent>,
    ) -> Result<Self, String> {
        let thread_loop = Arc::new(
            unsafe { ThreadLoopBox::new(Some("petal-camera-registry"), None) }
                .map_err(|e| format!("Camera PipeWire registry unavailable: {e:?}"))?,
        );
        thread_loop.start();
        // SAFETY: `loop_` lives inside an Arc this struct keeps alive, and
        // heap storage never moves. Every object created against this
        // reference is dropped in `Drop` before the Arc can go away.
        let loop_ref: &'static pipewire::loop_::Loop =
            unsafe { std::mem::transmute(thread_loop.loop_()) };
        let state = Arc::new(Mutex::new(RegistryState {
            devices: BTreeMap::new(),
            initial_pending: None,
            watch: None,
            lost: None,
            modes: None,
            seq_floor: 0,
        }));
        let (context, core, registry) = {
            let _guard = thread_loop.lock();
            let context = ContextBox::new(loop_ref, None)
                .map_err(|e| format!("Camera PipeWire context unavailable: {e:?}"))?;
            // SAFETY: the pw_context lives at a stable heap address owned by
            // this struct's ContextBox; moving the box never moves the
            // pointee, and Drop destroys it only after core and registry.
            let context_ref: &'static pw::context::Context =
                unsafe { std::mem::transmute(&*context) };
            let core = context_ref
                .connect_fd(remote, None)
                .map_err(|e| format!("Camera PipeWire remote rejected: {e:?}"))?;
            // SAFETY: same Arc-pinned-loop argument as `loop_ref` above; the
            // CoreBox lives in this struct and drops before the loop.
            let core_ref: &'static pw::core::Core = unsafe { std::mem::transmute(&*core) };
            let registry = core_ref
                .get_registry()
                .map_err(|e| format!("Camera PipeWire registry unavailable: {e:?}"))?;
            install_listeners(&registry, core_ref, state.clone(), events.clone());
            let seq = {
                let mut state = state.lock_unpoisoned();
                let seq = state.next_seq();
                state.initial_pending = Some(seq);
                seq
            };
            core_ref
                .sync(seq)
                .map_err(|e| format!("Camera PipeWire registry sync failed: {e:?}"))?;
            (context, core, registry)
        };
        Ok(Self {
            loop_: thread_loop,
            context: Some(context),
            core: Some(core),
            registry: Some(registry),
            state,
            events,
        })
    }

    /// Wait for the initial registry enumeration to finish. Fails closed on
    /// timeout with an explicit unavailable message.
    pub fn wait_initial(&self) -> Result<(), String> {
        let deadline = std::time::Instant::now() + REMOTE_ANSWER_TIMEOUT;
        loop {
            {
                let state = self.state.lock_unpoisoned();
                if let Some(error) = &state.lost {
                    return Err(error.clone());
                }
                if state.initial_pending.is_none() {
                    if state.devices.is_empty() {
                        // A granted remote with no camera nodes is an honest
                        // "no camera available", not an invented device.
                        return Err("No camera is available".to_string());
                    }
                    return Ok(());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(
                    "The camera list did not arrive from the desktop portal remote".to_string(),
                );
            }
            self.loop_.timed_wait(WAIT_SLICE);
        }
    }

    pub fn devices(&self) -> Vec<RegistryDevice> {
        self.state
            .lock_unpoisoned()
            .devices
            .iter()
            .map(|(node, label)| RegistryDevice {
                node: *node,
                label: label.clone(),
            })
            .collect()
    }

    /// The node whose registry removal invalidates the active selection.
    pub fn watch(&self, node: Option<u32>) {
        self.state.lock_unpoisoned().watch = node;
    }

    /// Enumerate one node's real SPA_PARAM_EnumFormat modes. Called from any
    /// thread; the reply arrives on the loop thread while this one waits.
    pub fn modes(&self, node: u32) -> Result<Vec<CameraMode>, String> {
        let seq = {
            let mut state = self.state.lock_unpoisoned();
            if let Some(error) = &state.lost {
                return Err(error.clone());
            }
            if !state.devices.contains_key(&node) {
                return Err("Selected camera is no longer available".to_string());
            }
            let seq = state.next_seq();
            state.modes = Some(ModeRequest {
                node,
                seq,
                done: false,
            });
            seq
        };
        let (send, receive) = std::sync::mpsc::channel::<RawFormat>();
        // Both stay alive until the reply lands (or fails): the listener's
        // Drop removes the hook and the proxy's Drop destroys the pw_proxy,
        // both under the loop lock below.
        let mut listener = None;
        let mut bound_proxy = None;
        {
            let _guard = self.loop_.lock();
            let registry = self.registry.as_ref().ok_or("Camera registry retired")?;
            // SAFETY: registry lives in this struct, dropped under the same
            // loop lock after this borrow ends.
            let registry_ref: &'static Registry = unsafe { std::mem::transmute(&**registry) };
            let node_proxy = bind_node(registry_ref, node)?;
            let parsed = send.clone();
            listener = Some(
                node_proxy
                    .add_listener_local()
                    .param(move |_seq, id, _index, _next, pod| {
                        // The pod is only valid during the callback: parse it
                        // now, never store it.
                        if id != spa::param::ParamType::EnumFormat {
                            return;
                        }
                        if let Some(format) = pod.and_then(parse_enum_format) {
                            let _ = parsed.send(RawFormat {
                                sizes: format.sizes,
                                rates: format.rates,
                            });
                        }
                    })
                    .register(),
            );
            node_proxy.enum_params(seq, Some(spa::param::ParamType::EnumFormat), 0, u32::MAX);
            bound_proxy = Some(node_proxy);
        }
        let outcome = self.await_modes(receive, deadline());
        // Retire the enumeration's PipeWire objects under the loop lock.
        {
            let _guard = self.loop_.lock();
            drop(listener);
            drop(bound_proxy);
        }
        self.state.lock_unpoisoned().modes = None;
        outcome
    }

    fn await_modes(
        &self,
        receive: std::sync::mpsc::Receiver<RawFormat>,
        deadline: std::time::Instant,
    ) -> Result<Vec<CameraMode>, String> {
        let mut formats = Vec::new();
        loop {
            match receive.recv_timeout(WAIT_SLICE) {
                Ok(format) => formats.push(format),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {}
            }
            {
                let state = self.state.lock_unpoisoned();
                if let Some(error) = &state.lost {
                    return Err(error.clone());
                }
                match &state.modes {
                    Some(request) if request.done => {
                        return Ok(modes_from_formats(formats));
                    }
                    Some(_) => {}
                    None => return Err("Camera mode enumeration was superseded".to_string()),
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("The camera's supported modes did not arrive".to_string());
            }
        }
    }

    pub fn events(&self) -> &std::sync::mpsc::Sender<MetadataEvent> {
        &self.events
    }
}

fn deadline() -> std::time::Instant {
    std::time::Instant::now() + REMOTE_ANSWER_TIMEOUT
}

impl Drop for CameraRegistry {
    fn drop(&mut self) {
        // Stop dispatching, then destroy the PipeWire objects while the loop
        // they borrow is still alive.
        let _guard = self.loop_.lock();
        self.loop_.stop();
        self.registry.take();
        self.core.take();
        self.context.take();
    }
}

impl RegistryState {
    fn next_seq(&mut self) -> i32 {
        let seq = self.seq_floor + 1;
        self.seq_floor = seq;
        seq
    }
}

/// Bind a node proxy by global id without keeping every GlobalObject alive:
/// `Registry::bind` only reads id/type/version, so a props-less view of the
/// stored node identity is sufficient and honest.
fn bind_node(registry: &Registry, id: u32) -> Result<pw::node::Node, String> {
    let global = pipewire::registry::GlobalObject::<&spa::utils::dict::DictRef> {
        id,
        permissions: pipewire::permissions::PermissionFlags::all(),
        type_: pipewire::types::ObjectType::Node,
        version: pw::sys::PW_VERSION_NODE,
        props: None,
    };
    registry
        .bind(&global)
        .map_err(|e| format!("Camera node bind failed: {e:?}"))
}

/// Shared view of the pod header (size/type) without copying the pod.
fn raw_pod(pod: &Pod) -> &sys::spa_pod {
    unsafe { &*pod.as_raw_ptr() }
}

fn install_listeners(
    registry: &pipewire::registry::RegistryBox,
    core: &pw::core::Core,
    state: Arc<Mutex<RegistryState>>,
    events: std::sync::mpsc::Sender<MetadataEvent>,
) {
    let registry_state = state.clone();
    let registry_events = events.clone();
    let registry_remove_state = state.clone();
    let registry_remove_events = events.clone();
    let registry_node_removed = events.clone();
    registry
        .add_listener_local()
        .global(move |global| {
            let Some(props) = global.props else { return };
            if global.type_ != pipewire::types::ObjectType::Node {
                return;
            }
            let media_class = props.get("media.class").unwrap_or_default();
            if !media_class.starts_with("Video/Source") {
                return;
            }
            let label = device_label(global.id, props);
            let mut state = registry_state.lock_unpoisoned();
            state.devices.insert(global.id, label);
            publish_devices(&state, &registry_events);
        })
        .global_remove(move |id| {
            let mut state = registry_remove_state.lock_unpoisoned();
            let was_camera = state.devices.remove(&id).is_some();
            publish_devices(&state, &registry_remove_events);
            if was_camera && state.watch == Some(id) {
                let _ = registry_node_removed.send(MetadataEvent::NodeRemoved(id));
            }
        })
        .register();

    let core_state = state.clone();
    let core_events = events.clone();
    let core_error_state = state.clone();
    let core_error_events = events.clone();
    core.add_listener_local()
        .done(move |id, seq| {
            let seq = seq.seq();
            if id != pw::core::PW_ID_CORE {
                return;
            }
            let mut state = core_state.lock_unpoisoned();
            if state.initial_pending == Some(seq) {
                state.initial_pending = None;
            }
            if let Some(request) = state.modes.as_mut() {
                if request.seq == seq {
                    request.done = true;
                }
            }
        })
        .error(move |_id, _seq, res, message| {
            let core_state = &core_error_state;
            let core_events = &core_error_events;
            let text = if res == -32 {
                // -EPIPE: the remote fd closed under us.
                format!("Camera PipeWire remote disconnected: {message}")
            } else {
                format!("Camera PipeWire remote error ({res}): {message}")
            };
            let mut state = core_state.lock_unpoisoned();
            if state.lost.is_none() {
                state.lost = Some(text.clone());
                if let Some(request) = state.modes.as_mut() {
                    request.done = true;
                }
                let _ = core_events.send(MetadataEvent::AccessLost(text));
            }
        })
        .register();
}

fn publish_devices(state: &RegistryState, events: &std::sync::mpsc::Sender<MetadataEvent>) {
    let devices = state
        .devices
        .iter()
        .map(|(node, label)| RegistryDevice {
            node: *node,
            label: label.clone(),
        })
        .collect();
    let _ = events.send(MetadataEvent::Devices(devices));
}

/// Labels derive from actual node properties; the fallback still names the
/// real node id instead of guessing a device path.
fn device_label(id: u32, props: &DictRef) -> String {
    for key in ["node.description", "node.nick", "node.name"] {
        if let Some(value) = props.get(key) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    format!("Camera {id}")
}

// ---------------------------------------------------------------------------
// EnumFormat parsing: pure functions over SPA pods, unit-testable below.
// ---------------------------------------------------------------------------

/// One parsed EnumFormat object: raw-video identification plus the concrete
/// sizes and rates its choice structures advertised.
struct ParsedEnumFormat {
    video: bool,
    sizes: Vec<(u32, u32)>,
    rates: Vec<(u32, u32)>,
}

fn parse_enum_format(pod: &Pod) -> Option<ParsedEnumFormat> {
    let object = <&PodObject>::try_from(pod).ok()?;
    let mut media_type = None;
    let mut media_subtype = None;
    let mut format = ParsedEnumFormat {
        video: false,
        sizes: Vec::new(),
        rates: Vec::new(),
    };
    for prop in object.props() {
        match prop.key().0 {
            sys::SPA_FORMAT_mediaType => {
                media_type = fixed_ids(prop.value()).first().copied();
            }
            sys::SPA_FORMAT_mediaSubtype => {
                media_subtype = fixed_ids(prop.value()).first().copied();
            }
            sys::SPA_FORMAT_VIDEO_size => {
                format.sizes = choice_rectangles(prop.value());
            }
            sys::SPA_FORMAT_VIDEO_framerate => {
                format.rates = choice_fractions(prop.value());
            }
            _ => {}
        }
    }
    format.video = media_type == Some(sys::SPA_MEDIA_TYPE_video)
        && media_subtype == Some(sys::SPA_MEDIA_SUBTYPE_raw);
    format.video.then_some(format)
}

/// Fixed-pod values of an Id pod or a choice of Ids.
fn fixed_ids(pod: &Pod) -> Vec<u32> {
    let raw = raw_pod(pod);
    match raw.type_ {
        sys::SPA_TYPE_Id => pod_body(raw)
            .map(|body| read_u32(body, raw.size))
            .flatten()
            .into_iter()
            .collect(),
        sys::SPA_TYPE_Choice => choice_alternatives(pod)
            .filter(|(child, _)| child.type_ == sys::SPA_TYPE_Id)
            .filter_map(|(child, body)| read_u32(body, child.size))
            .collect(),
        _ => Vec::new(),
    }
}

/// Concrete rectangles from a fixed Rectangle or a choice of them.
///
/// SPA Enum/Flags choices carry genuinely distinct alternatives; Range/Step
/// choices mean continuous support, so both endpoints are advertised (any
/// interior size is supported but listing the full continuum is unusable).
fn choice_rectangles(pod: &Pod) -> Vec<(u32, u32)> {
    let raw = raw_pod(pod);
    match raw.type_ {
        sys::SPA_TYPE_Rectangle => fixed_rectangle(raw).into_iter().collect(),
        sys::SPA_TYPE_Choice => {
            let choice = choice_type(pod);
            let mut values: Vec<(u32, u32)> = choice_alternatives(pod)
                .filter(|(child, _)| child.type_ == sys::SPA_TYPE_Rectangle)
                .filter_map(|(child, body)| {
                    let width = read_u32(body, child.size)?;
                    let height = read_u32(unsafe { body.add(4) }, child.size.saturating_sub(4))?;
                    Some((width, height))
                })
                .collect();
            if matches!(choice, sys::SPA_CHOICE_Range | sys::SPA_CHOICE_Step) && values.len() >= 2 {
                // Children after the default: min, max[, step]. Advertise the
                // genuine bounds, not the default alone.
                values = vec![values[0], values[1]];
            }
            values
        }
        _ => Vec::new(),
    }
}

/// Concrete frame rates from a fixed Fraction or a choice of them. A rate
/// range is advertised by its maximum: the camera delivers up to that rate.
fn choice_fractions(pod: &Pod) -> Vec<(u32, u32)> {
    let raw = raw_pod(pod);
    match raw.type_ {
        sys::SPA_TYPE_Fraction => fixed_fraction(raw).into_iter().collect(),
        sys::SPA_TYPE_Choice => {
            let choice = choice_type(pod);
            let mut values: Vec<(u32, u32)> = choice_alternatives(pod)
                .filter(|(child, _)| child.type_ == sys::SPA_TYPE_Fraction)
                .filter_map(|(child, body)| {
                    (child.size as usize >= 8)
                        .then(|| {
                            let num = read_u32(body, child.size)?;
                            let den =
                                read_u32(unsafe { body.add(4) }, child.size.saturating_sub(4))?;
                            (den > 0).then_some((num, den))
                        })
                        .flatten()
                })
                .collect();
            if matches!(choice, sys::SPA_CHOICE_Range | sys::SPA_CHOICE_Step) && values.len() >= 2 {
                // Children after the default: min, max[, step]. Advertise the
                // maximum rate the camera can deliver.
                values = vec![values[1]];
            }
            values
        }
        _ => Vec::new(),
    }
}

fn choice_type(pod: &Pod) -> u32 {
    debug_assert_eq!(raw_pod(pod).type_, sys::SPA_TYPE_Choice);
    // spa_pod_choice = { spa_pod pod; { u32 type; u32 flags; spa_pod child; } body; }
    unsafe { (*pod.as_raw_ptr().cast::<sys::spa_pod_choice>()).body.type_ }
}

/// Iterate a choice pod's advertised values: SPA choice children carry the
/// default first and the genuine alternatives after it, so the default
/// duplicate is skipped. A single-value (None) choice keeps its one child.
fn choice_alternatives(pod: &Pod) -> impl Iterator<Item = (&sys::spa_pod, *const u8)> {
    let skip_default = choice_type(pod) != sys::SPA_CHOICE_None;
    let mut children = choice_children(pod).peekable();
    if skip_default {
        children.next();
    }
    children
}

/// Iterate a choice pod's children: (child header reference, child body ptr).
/// The child headers live inside the choice pod's own allocation, so their
/// lifetime is the pod's.
fn choice_children(pod: &Pod) -> impl Iterator<Item = (&sys::spa_pod, *const u8)> {
    let raw = raw_pod(pod);
    let children_start = unsafe {
        let choice = (raw as *const sys::spa_pod).cast::<sys::spa_pod_choice>();
        std::ptr::addr_of!((*choice).body.child) as *const u8
    };
    let total = raw.size as usize;
    let mut offset = 0usize;
    std::iter::from_fn(move || {
        if offset + std::mem::size_of::<sys::spa_pod>() > total {
            return None;
        }
        unsafe {
            let header = &*children_start.add(offset).cast::<sys::spa_pod>();
            let size = header.size as usize;
            let body = children_start.add(offset + std::mem::size_of::<sys::spa_pod>());
            offset += round_up8(std::mem::size_of::<sys::spa_pod>() + size);
            if offset > total {
                return None;
            }
            Some((header, body))
        }
    })
}

fn round_up8(value: usize) -> usize {
    value.div_ceil(8) * 8
}

fn pod_body(pod: &sys::spa_pod) -> Option<*const u8> {
    (pod.size as usize >= 4).then(|| unsafe {
        std::ptr::addr_of!(pod.size)
            .add(2)
            .cast::<std::os::raw::c_void>()
            .cast::<u8>()
    })
}

fn read_u32(body: *const u8, size: u32) -> Option<u32> {
    (size as usize >= 4).then(|| unsafe { body.cast::<u32>().read_unaligned() })
}

fn fixed_rectangle(pod: &sys::spa_pod) -> Option<(u32, u32)> {
    let body = pod_body(pod)?;
    let width = read_u32(body, pod.size)?;
    let height = read_u32(unsafe { body.add(4) }, pod.size.saturating_sub(4))?;
    Some((width, height))
}

fn fixed_fraction(pod: &sys::spa_pod) -> Option<(u32, u32)> {
    let (num, den) = fixed_rectangle(pod)?;
    (den > 0).then_some((num, den))
}

/// Turn parsed EnumFormat entries into the user-facing mode list: every
/// distinct (size × rate) pair, deduplicated and sorted.
fn modes_from_formats(formats: Vec<RawFormat>) -> Vec<CameraMode> {
    let mut modes = Vec::new();
    for format in &formats {
        for &(width, height) in &format.sizes {
            if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
                // NV12 capture requires even dimensions; an odd advertised
                // mode is not honestly selectable.
                continue;
            }
            for &(num, den) in &format.rates {
                if num == 0 || den == 0 {
                    continue;
                }
                modes.push(CameraMode {
                    width,
                    height,
                    frame_rate_numerator: num,
                    frame_rate_denominator: den,
                });
            }
        }
    }
    modes.sort_by(|left, right| {
        (left.width, left.height)
            .cmp(&(right.width, right.height))
            .then_with(|| {
                let left_rate =
                    left.frame_rate_numerator as f64 / left.frame_rate_denominator.max(1) as f64;
                let right_rate =
                    right.frame_rate_numerator as f64 / right.frame_rate_denominator.max(1) as f64;
                right_rate
                    .partial_cmp(&left_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    modes.dedup();
    modes
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hand-built SPA pods: the binary format is a sequence of 8-byte-aligned
    // (u32 size, u32 type) headers followed by bodies, exactly what the
    // PipeWire remote delivers. Building them here keeps the parsers honest
    // without a live camera; the backing buffers stay owned by each test.
    fn pod_bytes(type_: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&type_.to_le_bytes());
        bytes.extend_from_slice(body);
        while bytes.len() % 8 != 0 {
            bytes.push(0);
        }
        bytes
    }

    /// Pod view over a buffer the caller keeps alive for the view's scope.
    fn pod_view(bytes: &[u8]) -> &Pod {
        assert!(bytes.len() >= 8);
        unsafe { &*(bytes.as_ptr().cast::<Pod>()) }
    }

    fn id_pod(value: u32) -> Vec<u8> {
        pod_bytes(sys::SPA_TYPE_Id, &value.to_le_bytes())
    }

    fn rectangle_pod(width: u32, height: u32) -> Vec<u8> {
        let body: Vec<u8> = [width, height]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        pod_bytes(sys::SPA_TYPE_Rectangle, &body)
    }

    fn fraction_pod(num: u32, den: u32) -> Vec<u8> {
        let body: Vec<u8> = [num, den].iter().flat_map(|v| v.to_le_bytes()).collect();
        pod_bytes(sys::SPA_TYPE_Fraction, &body)
    }

    fn choice_pod(choice: u32, children: &[Vec<u8>]) -> Vec<u8> {
        // spa_pod_choice_body = { u32 type, u32 flags, spa_pod child, ... }
        let mut body = Vec::new();
        body.extend_from_slice(&choice.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        let mut size = 8usize; // body prefix
        for child in children {
            size += child.len();
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        bytes.extend_from_slice(&sys::SPA_TYPE_Choice.to_le_bytes());
        bytes.extend_from_slice(&body);
        for child in children {
            bytes.extend_from_slice(child);
        }
        while bytes.len() % 8 != 0 {
            bytes.push(0);
        }
        bytes
    }

    fn prop(key: u32, value: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&key.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // flags
        bytes.extend_from_slice(value);
        bytes
    }

    fn format_pod_bytes(props: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&sys::SPA_TYPE_OBJECT_Format.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes()); // object id
        for prop in props {
            body.extend_from_slice(prop);
        }
        pod_bytes(sys::SPA_TYPE_Object, &body)
    }

    #[test]
    fn fixed_enum_format_parses_one_size_and_rate() {
        let object = format_pod_bytes(&[
            prop(
                sys::SPA_FORMAT_mediaType,
                &id_pod(sys::SPA_MEDIA_TYPE_video),
            ),
            prop(
                sys::SPA_FORMAT_mediaSubtype,
                &id_pod(sys::SPA_MEDIA_SUBTYPE_raw),
            ),
            prop(sys::SPA_FORMAT_VIDEO_size, &rectangle_pod(1280, 720)),
            prop(sys::SPA_FORMAT_VIDEO_framerate, &fraction_pod(30, 1)),
        ]);
        let parsed = parse_enum_format(pod_view(&object)).expect("video raw format");
        assert!(parsed.video);
        assert_eq!(parsed.sizes, vec![(1280, 720)]);
        assert_eq!(parsed.rates, vec![(30, 1)]);
    }

    #[test]
    fn enum_choices_list_every_alternative() {
        let sizes = choice_pod(
            sys::SPA_CHOICE_Enum,
            &[
                rectangle_pod(640, 480), // default
                rectangle_pod(640, 480),
                rectangle_pod(1280, 720),
                rectangle_pod(1920, 1080),
            ],
        );
        let rates = choice_pod(
            sys::SPA_CHOICE_Enum,
            &[
                fraction_pod(30, 1),
                fraction_pod(15, 1),
                fraction_pod(30, 1),
            ],
        );
        let object = format_pod_bytes(&[
            prop(
                sys::SPA_FORMAT_mediaType,
                &id_pod(sys::SPA_MEDIA_TYPE_video),
            ),
            prop(
                sys::SPA_FORMAT_mediaSubtype,
                &id_pod(sys::SPA_MEDIA_SUBTYPE_raw),
            ),
            prop(sys::SPA_FORMAT_VIDEO_size, &sizes),
            prop(sys::SPA_FORMAT_VIDEO_framerate, &rates),
        ]);
        let parsed = parse_enum_format(pod_view(&object)).expect("video raw format");
        assert_eq!(parsed.sizes, vec![(640, 480), (1280, 720), (1920, 1080)]);
        assert_eq!(parsed.rates, vec![(15, 1), (30, 1)]);
    }

    #[test]
    fn range_choices_advertise_real_endpoints_and_max_rate() {
        let sizes = choice_pod(
            sys::SPA_CHOICE_Range,
            &[
                rectangle_pod(1280, 720),  // default
                rectangle_pod(640, 480),   // min
                rectangle_pod(1920, 1080), // max
            ],
        );
        let rates = choice_pod(
            sys::SPA_CHOICE_Range,
            &[fraction_pod(30, 1), fraction_pod(1, 1), fraction_pod(60, 1)],
        );
        let object = format_pod_bytes(&[
            prop(
                sys::SPA_FORMAT_mediaType,
                &id_pod(sys::SPA_MEDIA_TYPE_video),
            ),
            prop(
                sys::SPA_FORMAT_mediaSubtype,
                &id_pod(sys::SPA_MEDIA_SUBTYPE_raw),
            ),
            prop(sys::SPA_FORMAT_VIDEO_size, &sizes),
            prop(sys::SPA_FORMAT_VIDEO_framerate, &rates),
        ]);
        let parsed = parse_enum_format(pod_view(&object)).expect("video raw format");
        assert_eq!(parsed.sizes, vec![(640, 480), (1920, 1080)]);
        assert_eq!(parsed.rates, vec![(60, 1)]);
    }

    #[test]
    fn non_video_formats_are_not_camera_modes() {
        let object = format_pod_bytes(&[
            prop(
                sys::SPA_FORMAT_mediaType,
                &id_pod(sys::SPA_MEDIA_TYPE_audio),
            ),
            prop(
                sys::SPA_FORMAT_mediaSubtype,
                &id_pod(sys::SPA_MEDIA_SUBTYPE_raw),
            ),
            prop(sys::SPA_FORMAT_VIDEO_size, &rectangle_pod(1280, 720)),
            prop(sys::SPA_FORMAT_VIDEO_framerate, &fraction_pod(30, 1)),
        ]);
        assert!(parse_enum_format(pod_view(&object)).is_none());
    }

    #[test]
    fn odd_sizes_and_zero_denominators_are_dropped_and_modes_dedupe() {
        let formats = vec![
            RawFormat {
                sizes: vec![(640, 481), (640, 480), (641, 480)],
                rates: vec![(30, 1), (0, 0), (30, 2)],
            },
            RawFormat {
                sizes: vec![(640, 480)],
                rates: vec![(30, 1)],
            },
        ];
        let modes = modes_from_formats(formats);
        assert_eq!(
            modes,
            vec![
                CameraMode {
                    width: 640,
                    height: 480,
                    frame_rate_numerator: 30,
                    frame_rate_denominator: 1
                },
                CameraMode {
                    width: 640,
                    height: 480,
                    frame_rate_numerator: 30,
                    frame_rate_denominator: 2
                },
            ]
        );
    }

    #[test]
    fn a_choice_of_ids_reads_every_alternative() {
        let ids = choice_pod(
            sys::SPA_CHOICE_Enum,
            &[id_pod(1), id_pod(1), id_pod(7), id_pod(9)],
        );
        assert_eq!(fixed_ids(pod_view(&ids)), vec![1, 7, 9]);
    }

    #[test]
    fn device_labels_prefer_real_properties_over_a_generic_fallback() {
        use pipewire::spa::sys as spa_sys;
        let dict = spa::static_dict! {
            "node.description" => "   ",
            "node.name" => "v4l2_input.camera.0",
        };
        let dict_ref: &spa::utils::dict::DictRef = dict.as_ref();
        assert_eq!(device_label(42, dict_ref), "v4l2_input.camera.0");
        let empty = spa::static_dict! {
            "media.class" => "Video/Source",
        };
        // A camera node without any usable name property falls back to the
        // real node id, never a guessed device path.
        assert_eq!(device_label(7, empty.as_ref()), "Camera 7");
    }

    #[test]
    fn registry_sequence_numbers_never_collide_between_users() {
        let mut state = RegistryState::default();
        let first = state.next_seq();
        let second = state.next_seq();
        state.initial_pending = Some(second);
        let third = state.next_seq();
        state.modes = Some(ModeRequest {
            node: 1,
            seq: third,
            done: false,
        });
        let fourth = state.next_seq();
        let all = [first, second, third, fourth];
        let mut unique = all.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "sequences must stay distinct");
        assert!(fourth > third && third > second && second > first);
    }
}
