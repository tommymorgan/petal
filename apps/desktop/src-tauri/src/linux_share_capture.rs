//! Restricted PipeWire remote -> latest native BGRA sample, with no webview media.

use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use tokio::sync::watch;

use crate::capture::{CaptureBufferPool, CapturedFrame, CapturedFramePayload, PooledFrameData};
use crate::sync_ext::MutexExt;
use crate::video_color::VideoColorProfile;

pub(crate) const MIN_GSTREAMER_VERSION: (u32, u32) = (1, 28);
// The same factories requested by the monitor pipeline below. Registry-only
// inspection of these names must never construct a capture element.
pub(crate) const REQUIRED_CAPTURE_ELEMENTS: &[&str] = &[
    "pipewiresrc",
    "videoconvert",
    "videoflip",
    "videorate",
    "capsfilter",
    "appsink",
];

pub(super) struct CapturePipeline {
    pipeline: gst::Pipeline,
    bus: gst::Bus,
    // The exact remote outlives pipewiresrc, including its transition to NULL.
    _remote: OwnedFd,
    samples: watch::Receiver<Option<gst::Sample>>,
    pool: CaptureBufferPool,
    sequence: u64,
    logical_size: Option<(i32, i32)>,
}

impl CapturePipeline {
    pub fn open(
        remote: OwnedFd,
        node: u32,
        logical_size: Option<(i32, i32)>,
    ) -> Result<Self, String> {
        gst::init().map_err(|e| format!("GStreamer capture unavailable: {e}"))?;
        let (major, minor, _, _) = gst::version();
        if (major, minor) < MIN_GSTREAMER_VERSION {
            return Err("Native monitor capture requires GStreamer 1.28 or newer for bounded latest-frame queues".to_string());
        }
        let element = |name: &str| {
            gst::ElementFactory::make(name).build()
            .map_err(|_| format!("Required native capture component {name} is unavailable; screen sharing cannot start"))
        };
        let source = element("pipewiresrc")?;
        source.set_property("fd", remote.as_raw_fd());
        // ScreenCast v5 gives a node ID scoped to this remote, not a v6 serial.
        source.set_property("path", node.to_string());
        source.set_property_from_str("on-disconnect", "error");
        source.set_property("keepalive-time", 0i32);
        source.set_property("resend-last", false);
        source.set_property("max-buffers", 3i32);
        let convert = element("videoconvert")?;
        let orientation = element("videoflip")?;
        orientation.set_property_from_str("video-direction", "auto");
        let rate = element("videorate")?;
        rate.set_property("drop-only", true);
        let filter = element("capsfilter")?;
        filter.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "BGRA")
                .field("colorimetry", "sRGB")
                .field("framerate", gst::Fraction::new(30, 1))
                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                .build(),
        );
        let sink = element("appsink")?
            .downcast::<gst_app::AppSink>()
            .map_err(|_| "Native capture appsink has an invalid type")?;
        // GStreamer 1.28 target: both the appsink and application mailbox keep
        // only the newest pending sample. There is no intermediate queue.
        sink.set_property("max-buffers", 1u32);
        sink.set_property_from_str("leaky-type", "downstream");
        sink.set_property("sync", false);
        sink.set_property("wait-on-eos", false);
        sink.set_property("enable-last-sample", false);
        let (send, samples) = watch::channel(None);
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    if send.is_closed() {
                        return Err(gst::FlowError::Flushing);
                    }
                    send.send_replace(Some(sample));
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let pipeline = gst::Pipeline::new();
        let elements = [
            &source,
            &convert,
            &orientation,
            &rate,
            &filter,
            sink.upcast_ref(),
        ];
        pipeline
            .add_many(elements)
            .map_err(|e| format!("Cannot assemble native capture: {e}"))?;
        gst::Element::link_many(elements)
            .map_err(|e| format!("Cannot negotiate native capture: {e}"))?;
        let bus = pipeline.bus().ok_or("Native capture has no lifetime bus")?;
        let capture = Self {
            pipeline,
            bus,
            _remote: remote,
            samples,
            pool: Arc::new(Mutex::new(Vec::new())),
            sequence: 0,
            logical_size,
        };
        capture
            .pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| format!("Cannot start restricted PipeWire capture: {e}"))?;
        Ok(capture)
    }

    pub async fn next_frame(&mut self) -> Result<CapturedFrame, String> {
        loop {
            if let Some(message) = self
                .bus
                .pop_filtered(&[gst::MessageType::Error, gst::MessageType::Eos])
            {
                return Err(match message.view() {
                    gst::MessageView::Error(error) => {
                        format!("Monitor capture stopped: {}", error.error())
                    }
                    _ => "Monitor capture ended; select Share explicitly to retry".to_string(),
                });
            }
            tokio::select! {
                changed = self.samples.changed() => {
                    changed.map_err(|_| "Monitor frame source disconnected")?;
                    let sample = self.samples.borrow_and_update().clone()
                        .ok_or("Monitor capture returned an empty sample")?;
                    return self.copy_frame(&sample);
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    }

    fn copy_frame(&mut self, sample: &gst::Sample) -> Result<CapturedFrame, String> {
        let caps = sample
            .caps()
            .ok_or("Monitor frame has no negotiated format")?;
        let info = gst_video::VideoInfo::from_caps(caps)
            .map_err(|e| format!("Invalid monitor frame format: {e}"))?;
        if info.format() != gst_video::VideoFormat::Bgra
            || info.width() == 0
            || info.height() == 0
            || info.width() > 16384
            || info.height() > 16384
        {
            return Err("Monitor capture did not negotiate a usable BGRA picture".to_string());
        }
        let buffer = sample.buffer().ok_or("Monitor frame has no buffer")?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
            .map_err(|e| format!("Monitor frame is not readable: {e}"))?;
        let stride = *frame
            .plane_stride()
            .first()
            .ok_or("Monitor frame has no row stride")?;
        let row = (info.width() as usize)
            .checked_mul(4)
            .ok_or("Monitor frame size overflow")?;
        let size = row
            .checked_mul(info.height() as usize)
            .filter(|size| *size <= 128 * 1024 * 1024)
            .ok_or("Monitor frame exceeds the native capture size limit")?;
        if stride < 0 || (stride as usize) < row {
            return Err("Invalid monitor frame stride".to_string());
        }
        let pixels = frame
            .plane_data(0)
            .map_err(|e| format!("Monitor frame plane unavailable: {e}"))?;
        let required = (info.height() as usize - 1)
            .checked_mul(stride as usize)
            .and_then(|offset| offset.checked_add(row))
            .ok_or("Monitor frame layout overflow")?;
        if pixels.len() < required {
            return Err("Truncated monitor frame".to_string());
        }
        let mut bytes = self.pool.lock_unpoisoned().pop().unwrap_or_default();
        bytes.resize(size, 0);
        for (y, destination) in bytes.chunks_exact_mut(row).enumerate() {
            let offset = y * stride as usize;
            destination.copy_from_slice(&pixels[offset..offset + row]);
        }
        self.sequence += 1;
        let scale = self
            .logical_size
            .filter(|(w, h)| *w > 0 && *h > 0)
            .map(|(w, _)| info.width() as f64 / w as f64)
            .unwrap_or(1.0);
        Ok(CapturedFrame {
            width: info.width(),
            height: info.height(),
            payload: CapturedFramePayload::Bgra {
                data: PooledFrameData::with_pool(bytes, self.pool.clone()),
                bytes_per_row: row,
            },
            source_scale: scale,
            layout_validated: true,
            color_profile: VideoColorProfile::SRGB_BT709_FULL,
            sequence: self.sequence,
            dirty_rect_count: 0,
            dirty_area_px: 0,
            dirty_rects_known: false,
            lock_copy_ms: 0.0,
            region_generation: None,
        })
    }
}

impl Drop for CapturePipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
