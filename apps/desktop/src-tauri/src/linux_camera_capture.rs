//! Restricted Camera-portal PipeWire remote -> latest native NV12 sample.
//! Mirrors linux_share_capture.rs: bounded appsink, fail-closed errors, no
//! webview media and no second media participant.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use tokio::sync::watch;

use crate::sync_ext::MutexExt;
use crate::transport::camera::{CameraFrame, PreferredCameraMode};

pub(crate) const MIN_GSTREAMER_VERSION: (u32, u32) = (1, 28);
// The same factories requested by the camera pipeline below. Registry-only
// inspection of these names must never construct a capture element.
pub(crate) const REQUIRED_CAMERA_ELEMENTS: &[&str] = &[
    "pipewiresrc",
    "videoconvert",
    "videoscale",
    "videoflip",
    "videorate",
    "capsfilter",
    "appsink",
];

/// Per-lease status shared with the low-level capture: the neutral access or
/// device-loss explanation, plus a per-lease identity for `same_capture`.
pub(super) struct CameraCaptureStatus {
    terminal: Mutex<Option<String>>,
    frames: AtomicU64,
    /// Bumped when the underlying capture ends for any reason; a lease that
    /// reads it can no longer resurrect tracks or preview surfaces.
    retired: AtomicBool,
}

impl CameraCaptureStatus {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            terminal: Mutex::new(None),
            frames: AtomicU64::new(0),
            retired: AtomicBool::new(false),
        })
    }

    pub(super) fn retire(&self, reason: Option<String>) {
        self.retired.store(true, Ordering::SeqCst);
        if let Some(reason) = reason {
            let mut terminal = self.terminal.lock_unpoisoned();
            if terminal.is_none() {
                *terminal = Some(reason);
            }
        }
    }

    pub(super) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst)
    }

    pub(super) fn terminal_error(&self) -> Option<String> {
        self.terminal.lock_unpoisoned().clone()
    }

    fn delivered(&self) {
        self.frames.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn frames_delivered(&self) -> u64 {
        self.frames.load(Ordering::SeqCst)
    }
}

/// One bounded GStreamer capture on the portal's restricted remote. Frames
/// land in a single latest-wins slot; the owning coordinator pumps them.
pub(super) struct CameraCapturePipeline {
    pipeline: gst::Pipeline,
    bus: gst::Bus,
    // The exact remote outlives pipewiresrc, including its NULL transition.
    _remote: OwnedFd,
    samples: watch::Receiver<Option<gst::Sample>>,
    status: Arc<CameraCaptureStatus>,
    sequence: u64,
    /// Set once the bus carried an Error/EOS: the neutral camera-loss text.
    ended: AtomicBool,
}

impl CameraCapturePipeline {
    /// Open the pipeline for one selected node on its granted remote. The
    /// preferred mode, when supplied, must come from that node's advertised
    /// EnumFormat modes; it pins the negotiated size and rate.
    pub fn open(
        remote: OwnedFd,
        node: u32,
        preferred: Option<PreferredCameraMode>,
        status: Arc<CameraCaptureStatus>,
    ) -> Result<Self, String> {
        gst::init().map_err(|e| format!("GStreamer capture unavailable: {e}"))?;
        let (major, minor, _, _) = gst::version();
        if (major, minor) < MIN_GSTREAMER_VERSION {
            return Err(
                "Native camera capture requires GStreamer 1.28 or newer for bounded latest-frame queues"
                    .to_string(),
            );
        }
        let element = |name: &str| {
            gst::ElementFactory::make(name).build().map_err(|_| {
                format!("Required native camera component {name} is unavailable; the camera cannot start")
            })
        };
        let source = element("pipewiresrc")?;
        source.set_property("fd", remote.as_raw_fd());
        // Registry node ids are scoped to this remote's server connection.
        source.set_property("path", node.to_string());
        source.set_property_from_str("on-disconnect", "error");
        source.set_property("always-copy", true);
        let convert = element("videoconvert")?;
        let scale = element("videoscale")?;
        let orientation = element("videoflip")?;
        orientation.set_property_from_str("video-direction", "auto");
        let rate = element("videorate")?;
        rate.set_property("drop-only", true);
        let filter = element("capsfilter")?;
        let mut caps = gst::Caps::builder("video/x-raw").field("format", "NV12");
        if let Some(mode) = preferred {
            caps = caps
                .field("width", mode.width as i32)
                .field("height", mode.height as i32)
                .field("framerate", gst::Fraction::new(mode.frame_rate as i32, 1));
        }
        filter.set_property("caps", caps.build());
        let sink = element("appsink")?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| "Native camera appsink has an invalid type")?;
        // Both the appsink and the application mailbox keep only the newest
        // pending sample. There is no intermediate queue.
        sink.set_property("max-buffers", 1u32);
        sink.set_property_from_str("leaky-type", "downstream");
        sink.set_property("sync", false);
        sink.set_property("wait-on-eos", false);
        sink.set_property("enable-last-sample", false);
        let (send, samples) = watch::channel(None);
        let callback_status = status.clone();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if send.is_closed() {
                        return Err(gst::FlowError::Flushing);
                    }
                    callback_status.delivered();
                    send.send_replace(Some(sample));
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let pipeline = gst::Pipeline::new();
        let elements = [
            &source,
            &convert,
            &scale,
            &orientation,
            &rate,
            &filter,
            sink.upcast_ref(),
        ];
        pipeline
            .add_many(elements)
            .map_err(|e| format!("Cannot assemble native camera capture: {e}"))?;
        gst::Element::link_many(elements)
            .map_err(|e| format!("Cannot negotiate native camera capture: {e}"))?;
        let bus = pipeline
            .bus()
            .ok_or("Native camera capture has no lifetime bus")?;
        let capture = Self {
            pipeline,
            bus,
            _remote: remote,
            samples,
            status,
            sequence: 0,
            ended: AtomicBool::new(false),
        };
        capture
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("Cannot start restricted PipeWire camera capture: {e}"))?;
        Ok(capture)
    }

    /// Wait for the next frame. Pipeline Error/EOS retires the capture with
    /// the neutral camera-loss explanation from ADR 0003 — without an
    /// explicit portal denial there is no honest way to name a finer cause.
    pub async fn next_frame(&mut self) -> Result<CameraFrame, String> {
        loop {
            if !self.ended.load(Ordering::SeqCst) {
                if let Some(message) = self
                    .bus
                    .pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos])
                {
                    self.ended.store(true, Ordering::SeqCst);
                    let reason = match message.view() {
                        gst::MessageView::Error(error) => {
                            format!("Camera capture stopped: {}", error.error())
                        }
                        _ => "Camera capture ended".to_string(),
                    };
                    self.status.retire(Some(
                        "Camera access or device lost; turn the camera on again to retry"
                            .to_string(),
                    ));
                    log::warn!("linux camera capture ended: {reason}");
                    return Err(self.status.terminal_error().unwrap_or(reason));
                }
            }
            tokio::select! {
                changed = self.samples.changed() => {
                    changed.map_err(|_| "Camera frame source disconnected".to_string())?;
                    let sample = self.samples.borrow_and_update().clone()
                        .ok_or("Camera capture returned an empty sample".to_string())?;
                    return self.copy_frame(&sample);
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    }

    /// Copy one NV12 sample into the shared CameraFrame layout (tightly
    /// packed planes; GStreamer negotiated the strides).
    fn copy_frame(&mut self, sample: &gst::Sample) -> Result<CameraFrame, String> {
        let caps = sample
            .caps()
            .ok_or("Camera frame has no negotiated format")?;
        let info = gst_video::VideoInfo::from_caps(caps)
            .map_err(|e| format!("Invalid camera frame format: {e}"))?;
        if info.format() != gst_video::VideoFormat::Nv12
            || info.width() == 0
            || info.height() == 0
            || info.width() % 2 != 0
            || info.height() % 2 != 0
            || info.width() > 16384
            || info.height() > 16384
        {
            return Err("Camera capture did not negotiate a usable NV12 picture".to_string());
        }
        let buffer = sample.buffer().ok_or("Camera frame has no buffer")?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
            .map_err(|e| format!("Camera frame is not readable: {e}"))?;
        let width = info.width() as usize;
        let height = info.height() as usize;
        let y_plane = frame
            .plane_data(0)
            .map_err(|e| format!("Camera frame luma unavailable: {e}"))?;
        let uv_plane = frame
            .plane_data(1)
            .map_err(|e| format!("Camera frame chroma unavailable: {e}"))?;
        let y_stride = *frame
            .plane_stride()
            .first()
            .ok_or("Camera frame has no luma stride")?;
        let uv_stride = *frame
            .plane_stride()
            .get(1)
            .ok_or("Camera frame has no chroma stride")?;
        if y_stride <= 0 || uv_stride <= 0 {
            return Err("Invalid camera frame stride".to_string());
        }
        let y_stride = y_stride as usize;
        let uv_stride = uv_stride as usize;
        let y_len = width
            .checked_mul(height)
            .ok_or("Camera frame size overflow")?;
        let uv_len = width
            .checked_mul(height / 2)
            .ok_or("Camera frame size overflow")?;
        if y_plane.len() < y_stride * (height - 1) + width
            || uv_plane.len() < uv_stride * (height / 2 - 1) + width
        {
            return Err("Truncated camera frame".to_string());
        }
        let mut y = Vec::with_capacity(y_len);
        for row in 0..height {
            let offset = row * y_stride;
            y.extend_from_slice(&y_plane[offset..offset + width]);
        }
        let mut uv = Vec::with_capacity(uv_len);
        for row in 0..height / 2 {
            let offset = row * uv_stride;
            uv.extend_from_slice(&uv_plane[offset..offset + width]);
        }
        self.sequence += 1;
        let _ = self.sequence;
        Ok(CameraFrame {
            width: width as u32,
            height: height as u32,
            y,
            y_stride: width as u32,
            uv,
            uv_stride: width as u32,
            capture_wall_time_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
        })
    }

    pub(super) fn frames_delivered(&self) -> u64 {
        self.status.frames_delivered()
    }

    pub(super) fn status(&self) -> &Arc<CameraCaptureStatus> {
        &self.status
    }
}

impl Drop for CameraCapturePipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CameraCapturePipeline {
        gst::init().unwrap();
        // No source elements or capture permission: exercise the real native
        // sample adapter against negotiated GStreamer samples only.
        let pipeline = gst::Pipeline::new();
        let bus = pipeline.bus().unwrap();
        let (_, samples) = watch::channel(None);
        CameraCapturePipeline {
            pipeline,
            bus,
            _remote: std::fs::File::open("/dev/null").unwrap().into(),
            samples,
            status: CameraCaptureStatus::new(),
            sequence: 0,
            ended: AtomicBool::new(false),
        }
    }

    fn nv12_sample(width: i32, height: i32) -> gst::Sample {
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "NV12")
            .field("width", width)
            .field("height", height)
            .field("framerate", gst::Fraction::new(30, 1))
            .build();
        let y_len = (width * height) as usize;
        let uv_len = y_len / 2;
        let mut bytes = Vec::with_capacity(y_len + uv_len);
        bytes.extend((0..y_len).map(|i| (i % 251) as u8));
        bytes.extend((0..uv_len).map(|i| ((i * 7) % 241) as u8));
        let buffer = gst::Buffer::from_mut_slice(bytes);
        gst::Sample::builder().buffer(&buffer).caps(&caps).build()
    }

    #[test]
    fn nv12_samples_copy_tightly_packed_planes() {
        let mut capture = adapter();
        let frame = capture.copy_frame(&nv12_sample(8, 6)).unwrap();
        assert_eq!((frame.width, frame.height), (8, 6));
        assert_eq!(frame.y_stride, 8);
        assert_eq!(frame.uv_stride, 8);
        assert_eq!(frame.y.len(), 48);
        assert_eq!(frame.uv.len(), 24);
        assert_eq!(
            &frame.y[..8],
            &(0..8).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
        assert!(frame.capture_wall_time_us > 0);
    }

    #[test]
    fn non_nv12_or_odd_formats_are_rejected_without_guessing() {
        let mut capture = adapter();
        for (format, width, height) in [("BGRA", 6, 4), ("NV12", 7, 4), ("NV12", 6, 5)] {
            let caps = gst::Caps::builder("video/x-raw")
                .field("format", format)
                .field("width", width)
                .field("height", height)
                .field("framerate", gst::Fraction::new(30, 1))
                .build();
            let bytes = vec![0u8; (width * height * 4) as usize];
            let buffer = gst::Buffer::from_mut_slice(bytes);
            let sample = gst::Sample::builder().buffer(&buffer).caps(&caps).build();
            assert!(
                capture.copy_frame(&sample).is_err(),
                "{format} {width}x{height} must not become an NV12 camera frame"
            );
        }
    }

    #[tokio::test]
    async fn eos_retires_the_capture_with_the_neutral_loss_explanation() {
        let mut capture = adapter();
        capture.bus.post(gst::message::Eos::new()).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), capture.next_frame())
            .await
            .expect("EOS must promptly retire a lost camera source");
        let error = result.expect_err("an ended capture must never produce a frame");
        assert!(error.contains("Camera access or device lost"));
        assert!(capture.status().is_retired());
        assert_eq!(
            capture.status().terminal_error().as_deref(),
            Some(error.as_str())
        );
        // Retired stays retired: a late frame sample cannot resurrect it.
        capture.samples.mark_changed();
        let again = tokio::time::timeout(std::time::Duration::from_secs(1), capture.next_frame())
            .await
            .expect("retired capture must keep settling");
        assert!(again.is_err());
    }

    #[tokio::test]
    async fn pipeline_error_retires_with_the_same_neutral_explanation() {
        let mut capture = adapter();
        let error = gst::message::Error::new(gst::LibraryError::Failed, "camera node vanished");
        capture.bus.post(error).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), capture.next_frame())
            .await
            .expect("a failed camera capture must settle promptly");
        let message = result.expect_err("a failed capture must never produce a frame");
        assert!(message.contains("Camera access or device lost"));
        assert!(capture.status().is_retired());
    }

    #[test]
    fn lease_status_counts_delivered_frames() {
        let status = CameraCaptureStatus::new();
        assert_eq!(status.frames_delivered(), 0);
        status.delivered();
        status.delivered();
        assert_eq!(status.frames_delivered(), 2);
        status.retire(None);
        assert!(status.is_retired());
        assert_eq!(status.terminal_error(), None);
    }
}
