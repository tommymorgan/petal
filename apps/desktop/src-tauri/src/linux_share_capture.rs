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
// The same factories requested by the picture pipeline below. Registry-only
// inspection of these names must never construct a capture element.
pub(crate) const REQUIRED_CAPTURE_ELEMENTS: &[&str] = &[
    "pipewiresrc",
    "videoconvert",
    "videocrop",
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
}

impl CapturePipeline {
    pub fn open(remote: OwnedFd, node: u32) -> Result<Self, String> {
        gst::init().map_err(|e| format!("GStreamer capture unavailable: {e}"))?;
        let (major, minor, _, _) = gst::version();
        if (major, minor) < MIN_GSTREAMER_VERSION {
            return Err("Native picture capture requires GStreamer 1.28 or newer for bounded latest-frame queues".to_string());
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
        // A damage-driven window can become idle before its subscriber is
        // ready. Refresh the genuine last sample while connected, never on EOS.
        source.set_property("keepalive-time", 1000i32);
        source.set_property("resend-last", false);
        source.set_property("max-buffers", 3i32);
        let convert = element("videoconvert")?;
        let crop = element("videocrop")?;
        install_window_crop(&crop)?;
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
            &crop,
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
                        format!("Picture capture stopped: {}", error.error())
                    }
                    _ => "Picture capture ended; select Share explicitly to retry".to_string(),
                });
            }
            tokio::select! {
                changed = self.samples.changed() => {
                    changed.map_err(|_| "Picture frame source disconnected")?;
                    let sample = self.samples.borrow_and_update().clone()
                        .ok_or("Picture capture returned an empty sample")?;
                    return self.copy_frame(&sample);
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    }

    fn copy_frame(&mut self, sample: &gst::Sample) -> Result<CapturedFrame, String> {
        let caps = sample
            .caps()
            .ok_or("Picture frame has no negotiated format")?;
        let info = gst_video::VideoInfo::from_caps(caps)
            .map_err(|e| format!("Invalid picture frame format: {e}"))?;
        if info.format() != gst_video::VideoFormat::Bgra
            || info.width() == 0
            || info.height() == 0
            || info.width() > 16384
            || info.height() > 16384
        {
            return Err("Picture capture did not negotiate a usable BGRA picture".to_string());
        }
        let buffer = sample.buffer().ok_or("Picture frame has no buffer")?;
        // Mutter window streams can retain a monitor-sized allocation while
        // reporting the current window bounds on each buffer.
        let (x, y, width, height) = buffer
            .meta::<gst_video::VideoCropMeta>()
            .map(|meta| meta.rect())
            .unwrap_or((0, 0, info.width(), info.height()));
        if width == 0
            || height == 0
            || x.checked_add(width)
                .map_or(true, |right| right > info.width())
            || y.checked_add(height)
                .map_or(true, |bottom| bottom > info.height())
        {
            return Err("Invalid picture frame crop".to_string());
        }
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info)
            .map_err(|e| format!("Picture frame is not readable: {e}"))?;
        let stride = *frame
            .plane_stride()
            .first()
            .ok_or("Picture frame has no row stride")?;
        let row = (info.width() as usize)
            .checked_mul(4)
            .ok_or("Picture frame size overflow")?;
        row.checked_mul(info.height() as usize)
            .filter(|size| *size <= 128 * 1024 * 1024)
            .ok_or("Picture frame exceeds the native capture size limit")?;
        if stride < 0 || (stride as usize) < row {
            return Err("Invalid picture frame stride".to_string());
        }
        let pixels = frame
            .plane_data(0)
            .map_err(|e| format!("Picture frame plane unavailable: {e}"))?;
        let required = (info.height() as usize - 1)
            .checked_mul(stride as usize)
            .and_then(|offset| offset.checked_add(row))
            .ok_or("Picture frame layout overflow")?;
        if pixels.len() < required {
            return Err("Truncated picture frame".to_string());
        }
        let row = width as usize * 4;
        let size = row * height as usize;
        let origin = y as usize * stride as usize + x as usize * 4;
        let mut bytes = self.pool.lock_unpoisoned().pop().unwrap_or_default();
        bytes.resize(size, 0);
        for (y, destination) in bytes.chunks_exact_mut(row).enumerate() {
            let offset = origin + y * stride as usize;
            destination.copy_from_slice(&pixels[offset..offset + row]);
        }
        self.sequence += 1;
        Ok(CapturedFrame {
            width,
            height,
            payload: CapturedFramePayload::Bgra {
                data: PooledFrameData::with_pool(bytes, self.pool.clone()),
                bytes_per_row: row,
            },
            // Picture-only shares use selected pixel coordinates. Portal Start
            // logical size is a snapshot, not live geometry after resize/scale.
            source_scale: 1.0,
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

// GStreamer 1.28 videoflip consumes orientation-tagged metadata with a matrix
// transform, which VideoCropMeta does not implement. Consume the actual crop
// before rotation instead. This probe and videocrop run on the same streaming
// thread with no intervening queue: properties always describe this buffer,
// including after a resize or a return to uncropped monitor samples.
fn install_window_crop(crop: &gst::Element) -> Result<(), String> {
    let pad = crop
        .static_pad("sink")
        .ok_or("Native crop has no sink pad")?;
    let weak_crop = crop.downgrade();
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, probe| {
        let Some(crop) = weak_crop.upgrade() else {
            return gst::PadProbeReturn::Drop;
        };
        let Some(buffer) = probe.buffer_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        match apply_window_crop(&crop, pad, buffer) {
            Ok(()) => gst::PadProbeReturn::Ok,
            Err(error) => {
                gst::element_error!(crop, gst::StreamError::Format, ("{error}"));
                gst::PadProbeReturn::Drop
            }
        }
    })
    .ok_or("Cannot install native window crop handling")?;
    Ok(())
}

fn apply_window_crop(
    crop: &gst::Element,
    pad: &gst::Pad,
    buffer: &mut gst::Buffer,
) -> Result<(), String> {
    let rect = buffer
        .meta::<gst_video::VideoCropMeta>()
        .map(|meta| meta.rect());
    let (left, top, right, bottom) = if let Some((x, y, width, height)) = rect {
        let caps = pad
            .current_caps()
            .ok_or("Picture crop has no negotiated format")?;
        let info = gst_video::VideoInfo::from_caps(&caps)
            .map_err(|e| format!("Invalid picture crop format: {e}"))?;
        if info.format() != gst_video::VideoFormat::Bgra
            || info.width() > 16384
            || info.height() > 16384
            || info.width() as u64 * info.height() as u64 * 4 > 128 * 1024 * 1024
            || width == 0
            || height == 0
            || x.checked_add(width)
                .map_or(true, |edge| edge > info.width())
            || y.checked_add(height)
                .map_or(true, |edge| edge > info.height())
        {
            return Err("Invalid picture frame crop".to_string());
        }
        // videocrop adds incoming crop-meta offsets to its configured offsets.
        // Consume the metadata so the native element crops exactly once.
        buffer
            .make_mut()
            .meta_mut::<gst_video::VideoCropMeta>()
            .ok_or("Picture crop metadata disappeared")?
            .remove()
            .map_err(|e| format!("Cannot consume picture crop metadata: {e}"))?;
        (
            x as i32,
            y as i32,
            (info.width() - x - width) as i32,
            (info.height() - y - height) as i32,
        )
    } else {
        (0, 0, 0, 0)
    };
    if (
        crop.property::<i32>("left"),
        crop.property::<i32>("top"),
        crop.property::<i32>("right"),
        crop.property::<i32>("bottom"),
    ) != (left, top, right, bottom)
    {
        crop.set_properties(&[
            ("left", &left),
            ("top", &top),
            ("right", &right),
            ("bottom", &bottom),
        ]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CapturePipeline {
        gst::init().unwrap();
        // No source elements or capture permission: exercise the real native
        // sample adapter against negotiated GStreamer samples only.
        let pipeline = gst::Pipeline::new();
        let bus = pipeline.bus().unwrap();
        let (_, samples) = watch::channel(None);
        CapturePipeline {
            pipeline,
            bus,
            _remote: std::fs::File::open("/dev/null").unwrap().into(),
            samples,
            pool: Arc::new(Mutex::new(Vec::new())),
            sequence: 0,
        }
    }

    fn sample(width: i32, height: i32, bytes: Vec<u8>) -> gst::Sample {
        let caps = gst::Caps::builder("video/x-raw")
            .field("format", "BGRA")
            .field("width", width)
            .field("height", height)
            .field("framerate", gst::Fraction::new(30, 1))
            .build();
        let buffer = gst::Buffer::from_mut_slice(bytes);
        gst::Sample::builder().buffer(&buffer).caps(&caps).build()
    }

    fn padded_sample(crop: Option<(u32, u32, u32, u32)>) -> gst::Sample {
        let sample = sample(5, 4, (0..96).collect());
        let mut buffer = sample.buffer_owned().unwrap();
        let buffer_ref = buffer.make_mut();
        gst_video::VideoMeta::add_full(
            buffer_ref,
            gst_video::VideoFrameFlags::empty(),
            gst_video::VideoFormat::Bgra,
            5,
            4,
            &[0],
            &[24],
        )
        .unwrap();
        if let Some(rect) = crop {
            gst_video::VideoCropMeta::add(buffer_ref, rect);
        }
        let caps = sample.caps().unwrap().to_owned();
        gst::Sample::builder().buffer(&buffer).caps(&caps).build()
    }

    #[test]
    fn window_crop_survives_native_color_orientation_and_rate_transforms() {
        for direction in ["auto", "90r"] {
            let mut capture = adapter();
            let pipeline = gst::parse_launch(&format!(
                "appsrc name=source format=time ! videoconvert ! videocrop name=crop ! videoflip video-direction={direction} ! videorate drop-only=true ! video/x-raw,format=BGRA,colorimetry=sRGB,framerate=30/1,pixel-aspect-ratio=1/1 ! appsink name=sink sync=false async=false",
            )).unwrap().downcast::<gst::Pipeline>().unwrap();
            install_window_crop(&pipeline.by_name("crop").unwrap()).unwrap();
            let source = pipeline
                .by_name("source")
                .unwrap()
                .downcast::<gst_app::AppSrc>()
                .unwrap();
            let sink = pipeline
                .by_name("sink")
                .unwrap()
                .downcast::<gst_app::AppSink>()
                .unwrap();
            // Same backing allocation, changing selected extent, edge pixel,
            // then no crop: stale crop properties must not leak between frames.
            let cases = [
                (
                    Some((1, 1, 3, 2)),
                    (3, 2),
                    vec![28, 32, 36, 52, 56, 60],
                    vec![52, 28, 56, 32, 60, 36],
                ),
                (
                    Some((0, 0, 2, 3)),
                    (2, 3),
                    vec![0, 4, 24, 28, 48, 52],
                    vec![48, 24, 0, 52, 28, 4],
                ),
                (Some((4, 3, 1, 1)), (1, 1), vec![88], vec![88]),
                (
                    None,
                    (5, 4),
                    vec![
                        0, 4, 8, 12, 16, 24, 28, 32, 36, 40, 48, 52, 56, 60, 64, 72, 76, 80, 84, 88,
                    ],
                    vec![
                        72, 48, 24, 0, 76, 52, 28, 4, 80, 56, 32, 8, 84, 60, 36, 12, 88, 64, 40, 16,
                    ],
                ),
            ];
            pipeline.set_state(gst::State::Playing).unwrap();
            let outputs: Vec<_> = cases
                .iter()
                .enumerate()
                .map(|(index, (crop, _, _, _))| {
                    let input = padded_sample(*crop);
                    let mut buffer = input.buffer_owned().unwrap();
                    buffer
                        .make_mut()
                        .set_pts(gst::ClockTime::from_seconds(index as u64));
                    buffer
                        .make_mut()
                        .set_duration(gst::ClockTime::from_nseconds(33_333_333));
                    let mut caps = input.caps().unwrap().to_owned();
                    caps.make_mut().set("colorimetry", "sRGB");
                    caps.make_mut()
                        .set("pixel-aspect-ratio", gst::Fraction::new(1, 1));
                    let input = gst::Sample::builder().buffer(&buffer).caps(&caps).build();
                    source.push_sample(&input).unwrap();
                    sink.try_pull_sample(gst::ClockTime::from_seconds(2))
                })
                .collect();
            source.end_of_stream().unwrap();
            let error = pipeline
                .bus()
                .unwrap()
                .pop_filtered(&[gst::MessageType::Error])
                .map(|message| match message.view() {
                    gst::MessageView::Error(error) => {
                        format!("{}: {:?}", error.error(), error.debug())
                    }
                    _ => unreachable!(),
                });
            // Retire native streaming threads before regression assertions.
            pipeline.set_state(gst::State::Null).unwrap();
            for (output, (_, (width, height), straight, rotated)) in outputs.into_iter().zip(cases)
            {
                let frame = capture
                    .copy_frame(&output.unwrap_or_else(|| {
                        panic!("Native transforms produced no picture; error: {error:?}")
                    }))
                    .unwrap();
                let (dimensions, offsets) = if direction == "auto" {
                    ((width, height), straight)
                } else {
                    ((height, width), rotated)
                };
                assert_eq!((frame.width, frame.height), dimensions, "{direction}");
                match &frame.payload {
                    CapturedFramePayload::Bgra {
                        data,
                        bytes_per_row,
                    } => {
                        assert_eq!(*bytes_per_row, dimensions.0 as usize * 4);
                        let expected: Vec<u8> = offsets
                            .into_iter()
                            .flat_map(|offset| offset..offset + 4)
                            .collect();
                        assert_eq!(&data[..], expected.as_slice(), "{direction}");
                    }
                    _ => panic!("Native transforms changed the BGRA payload kind"),
                }
            }
        }
    }

    #[test]
    fn cropped_window_samples_publish_only_selected_pixels_from_padded_rows() {
        let mut capture = adapter();
        let sample = padded_sample(Some((1, 1, 3, 2)));
        let frame = capture.copy_frame(&sample).unwrap();
        assert_eq!((frame.width, frame.height), (3, 2));
        assert_eq!(frame.source_scale, 1.0);
        assert_eq!(frame.sequence, 1);
        match &frame.payload {
            CapturedFramePayload::Bgra {
                data,
                bytes_per_row,
            } => {
                assert_eq!(*bytes_per_row, 12);
                let expected: Vec<u8> = (28..40).chain(52..64).collect();
                assert_eq!(&data[..], expected.as_slice());
            }
            _ => panic!("Selected BGRA sample changed payload kind"),
        }
        drop(frame);
        assert_eq!(
            capture.pool.lock_unpoisoned().len(),
            1,
            "Cropped storage must be reusable"
        );
    }

    #[test]
    fn uncropped_samples_publish_full_geometry_without_row_padding() {
        let mut capture = adapter();
        let frame = capture.copy_frame(&padded_sample(None)).unwrap();
        assert_eq!((frame.width, frame.height), (5, 4));
        match &frame.payload {
            CapturedFramePayload::Bgra {
                data,
                bytes_per_row,
            } => {
                assert_eq!(*bytes_per_row, 20);
                let expected: Vec<u8> = (0..20).chain(24..44).chain(48..68).chain(72..92).collect();
                assert_eq!(&data[..], expected.as_slice());
            }
            _ => panic!("Selected BGRA sample changed payload kind"),
        }
    }

    #[test]
    fn crops_outside_the_frame_or_with_empty_extent_are_rejected() {
        let mut capture = adapter();
        for crop in [
            (4, 1, 2, 2),
            (1, 3, 3, 2),
            (u32::MAX, 1, 2, 2),
            (1, u32::MAX, 3, 2),
            (1, 1, 0, 2),
            (1, 1, 3, 0),
        ] {
            assert!(
                capture.copy_frame(&padded_sample(Some(crop))).is_err(),
                "Accepted invalid crop {crop:?}"
            );
        }
        assert_eq!(capture.sequence, 0);
        assert!(capture.pool.lock_unpoisoned().is_empty());
    }

    #[test]
    fn resized_samples_preserve_selected_pixels_and_current_pixel_geometry() {
        let mut capture = adapter();
        for (index, (width, height)) in [(4, 2), (2, 4), (6, 2)].into_iter().enumerate() {
            let bytes: Vec<u8> = (0..width * height * 4)
                .map(|i| (i + index as i32) as u8)
                .collect();
            let sample = sample(width, height, bytes.clone());
            let frame = capture.copy_frame(&sample).unwrap();
            assert_eq!((frame.width, frame.height), (width as u32, height as u32));
            assert_eq!(frame.source_scale, 1.0);
            assert_eq!(frame.sequence, index as u64 + 1);
            match &frame.payload {
                CapturedFramePayload::Bgra {
                    data,
                    bytes_per_row,
                } => {
                    assert_eq!(*bytes_per_row, width as usize * 4);
                    assert_eq!(&data[..], bytes.as_slice());
                }
                _ => panic!("Selected BGRA sample changed payload kind"),
            }
        }
        assert_eq!(
            capture.pool.lock_unpoisoned().len(),
            1,
            "Sample storage must be reusable"
        );
    }

    #[test]
    fn truncated_selected_sample_is_rejected_instead_of_reusing_a_previous_frame() {
        let mut capture = adapter();
        let valid = sample(4, 2, vec![20; 32]);
        assert!(capture.copy_frame(&valid).is_ok());
        let truncated = sample(4, 2, vec![99; 4]);
        assert!(capture.copy_frame(&truncated).is_err());
        assert_eq!(capture.sequence, 1);
    }

    #[tokio::test]
    async fn source_eos_ends_capture_without_waiting_for_another_sample() {
        let mut capture = adapter();
        capture.bus.post(gst::message::Eos::new()).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), capture.next_frame())
            .await
            .expect("EOS must promptly retire a lost source");
        assert!(result.err().unwrap().contains("ended"));
    }
}
