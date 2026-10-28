//! Native Task 2.3 publisher, not a receiver or a compatibility substitute.
//!
//! Build on Windows: `cargo build --example windows_wgc_race_publisher`.
//! Run with `--backend-url URL --room-code CODE --hwnd DECIMAL --control-file PATH`.
//! The existing control file must initially be empty. Append newline-terminated
//! `publish-b`, `retire-a`, then `stop` commands in order; wait for each JSON
//! stdout event before advancing. All phases share a five-minute run budget.
//!
//! Uses normal backend admission, the picker target registry, WGC's System
//! indicator, and the production H.264 publisher. One capture session feeds A
//! until publish-b, then feeds only B: A remains published with its last real
//! image until retire-a. No generated pixels, audio, observer or server keys.
//! The operator owns source text/counter changes and receiver observations.
//! Ready events prove local source acceptance, NOT remote subscription/decode.

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("windows_wgc_race_publisher requires native Windows WGC");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create native publisher runtime");
    let result = runtime.block_on(native::run());
    // WGC setup/drop uses blocking workers. A hung native driver must not
    // make a failed fixture immortal after its bounded cleanup attempt.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    if let Err(error) = result {
        eprintln!("windows_wgc_race_publisher: {error}");
        std::process::exit(1);
    }
}

#[cfg(target_os = "windows")]
mod native {
    use desktop_lib::capture::{CapturedFrame, CapturedFramePayload};
    use desktop_lib::transport::publisher::{
        PublishedTrack, RoomConnection, ShareQuality, SharedSourceKind,
    };
    use desktop_lib::transport::token::{fetch_access_token, BackendTokenRequest};
    use desktop_lib::video_color::VideoColorProfile;
    use desktop_lib::windows_screen_capture::{
        BgraFrame, CaptureIndicatorMode, CaptureStatus, TargetCaptureSession,
    };
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
    };

    const RUN_BUDGET: Duration = Duration::from_secs(300);
    const STEP_BUDGET: Duration = Duration::from_secs(20);
    type Result<T, E = String> = std::result::Result<T, E>;

    struct Args {
        backend: String,
        room: String,
        hwnd: usize,
        control: PathBuf,
    }

    impl Args {
        fn parse() -> Result<Self> {
            let mut values = std::collections::HashMap::new();
            let mut args = std::env::args().skip(1);
            while let Some(key) = args.next() {
                if !matches!(
                    key.as_str(),
                    "--backend-url" | "--room-code" | "--hwnd" | "--control-file"
                ) {
                    return Err("expected --backend-url URL --room-code CODE --hwnd DECIMAL --control-file PATH".into());
                }
                let value = args.next().ok_or("missing option value")?;
                if value.trim().is_empty() || values.insert(key, value).is_some() {
                    return Err("empty or duplicate option".into());
                }
            }
            let backend = values
                .remove("--backend-url")
                .ok_or("missing --backend-url")?;
            let url = reqwest::Url::parse(&backend).map_err(|_| "invalid backend URL")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("backend URL must be an HTTP(S) base URL without credentials, query or fragment".into());
            }
            Ok(Self {
                backend,
                room: desktop_lib::rooms::internal_credential_for_access_code(
                    &values.remove("--room-code").ok_or("missing --room-code")?,
                )
                .ok_or("--room-code must be a native 3-4-3 access code")?,
                hwnd: values
                    .remove("--hwnd")
                    .ok_or("missing --hwnd")?
                    .parse()
                    .map_err(|_| "HWND must be pointer-sized unsigned decimal")?,
                control: values
                    .remove("--control-file")
                    .ok_or("missing --control-file")?
                    .into(),
            })
        }
    }

    fn hwnd_identity(raw: usize) -> Result<(u32, String)> {
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        if raw == 0 || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return Err("selected HWND is not live".into());
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let mut text = [0u16; 32768];
        let length = unsafe { GetWindowTextW(hwnd, &mut text) };
        if pid == 0 || length <= 0 {
            return Err("selected HWND must have a live owner and nonempty title".into());
        }
        Ok((pid, String::from_utf16_lossy(&text[..length as usize])))
    }

    fn verify_native_match(raw: usize, pid: u32, title: &str) -> Result<()> {
        struct Match<'a> {
            pid: u32,
            title: &'a str,
            raw: usize,
            count: usize,
            selected: bool,
        }
        unsafe extern "system" fn visit(hwnd: HWND, param: LPARAM) -> BOOL {
            let state = &mut *(param.0 as *mut Match<'_>);
            let mut owner = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut owner));
            if owner == state.pid {
                if let Ok((_, title)) = hwnd_identity(hwnd.0 as usize) {
                    if title == state.title {
                        state.count += 1;
                        state.selected |= hwnd.0 as usize == state.raw;
                    }
                }
            }
            BOOL(1)
        }
        let mut state = Match {
            pid,
            title,
            raw,
            count: 0,
            selected: false,
        };
        // Picker enumeration filters hidden/minimized/ineligible windows.
        // Check the unfiltered native set too: otherwise a matching eligible
        // sibling could silently stand in for the supplied HWND.
        unsafe { EnumWindows(Some(visit), LPARAM(&mut state as *mut _ as isize)) }
            .map_err(|_| "native HWND enumeration failed")?;
        if state.count != 1 || !state.selected {
            return Err(
                "selected HWND must uniquely own its process/title across all native windows"
                    .into(),
            );
        }
        Ok(())
    }

    fn resolve_target(raw: usize) -> Result<(u32, String)> {
        let (pid, title) = hwnd_identity(raw)?;
        verify_native_match(raw, pid, &title)?;
        let entries = desktop_lib::window_source::list()
            .map_err(|_| "production window enumeration failed")?;
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| {
                entry.app_pid as u32 == pid
                    && entry.title.as_deref() == Some(title.as_str())
                    && entry.kind == Some(desktop_lib::window_source::ShareableSourceKind::Window)
            })
            .collect();
        if matches.len() != 1 || hwnd_identity(raw)? != (pid, title.clone()) {
            return Err(
                "HWND must match exactly one stable picker window by owner PID and exact title"
                    .into(),
            );
        }
        verify_native_match(raw, pid, &title)?;
        Ok((matches[0].window_id, title))
    }

    struct Control {
        path: PathBuf,
        offset: u64,
        pending: String,
    }

    impl Control {
        fn open(path: PathBuf) -> Result<Self> {
            if std::fs::metadata(&path)
                .map_err(|_| "control file must already exist")?
                .len()
                != 0
            {
                return Err("control file must initially be empty".into());
            }
            Ok(Self {
                path,
                offset: 0,
                pending: String::new(),
            })
        }

        fn commands(&mut self) -> Result<Vec<String>> {
            let mut file =
                std::fs::File::open(&self.path).map_err(|_| "cannot open control file")?;
            if file
                .metadata()
                .map_err(|_| "cannot inspect control file")?
                .len()
                < self.offset
            {
                return Err("control file was truncated; append commands instead".into());
            }
            file.seek(SeekFrom::Start(self.offset))
                .map_err(|_| "cannot seek control file")?;
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read control file")?;
            if bytes.len() > 4096 || self.pending.len() + bytes.len() > 4096 {
                return Err("control command exceeds 4096 bytes".into());
            }
            self.offset += bytes.len() as u64;
            self.pending
                .push_str(std::str::from_utf8(&bytes).map_err(|_| "control file must be UTF-8")?);
            let mut commands = Vec::new();
            while let Some(end) = self.pending.find('\n') {
                let line: String = self.pending.drain(..=end).collect();
                let command = line.trim();
                if !command.is_empty() {
                    commands.push(command.to_string());
                }
            }
            Ok(commands)
        }
    }

    fn event(kind: &str, track: &PublishedTrack, token: u32) -> Result<()> {
        println!(
            "{}",
            serde_json::json!({
                "event": kind, "sid": track.sid().to_string(),
                "name": format!("petal-window-{token}"), "window_id": token
            })
        );
        std::io::stdout()
            .flush()
            .map_err(|_| "cannot flush fixture event".into())
    }

    fn push(track: &PublishedTrack, frame: BgraFrame, sequence: u64) -> bool {
        let timestamp = frame.capture_wall_time_us;
        let captured = CapturedFrame {
            width: frame.width,
            height: frame.height,
            payload: CapturedFramePayload::Bgra {
                data: frame.bgra.into(),
                bytes_per_row: frame.bytes_per_row,
            },
            source_scale: 1.0,
            layout_validated: true,
            color_profile: VideoColorProfile::SRGB_BT709_FULL,
            sequence,
            dirty_rect_count: 0,
            dirty_area_px: 0,
            dirty_rects_known: false,
            lock_copy_ms: 0.0,
            region_generation: frame.region_generation,
        };
        track
            .push_frame(&captured, timestamp)
            .is_some_and(|timing| timing.source_accepted)
    }

    struct Fixture {
        connection: Option<RoomConnection>,
        capture: Option<TargetCaptureSession>,
        a: Option<PublishedTrack>,
        b: Option<PublishedTrack>,
    }

    impl Fixture {
        async fn drive(&mut self, args: Args) -> Result<()> {
            let mut control = Control::open(args.control)?;
            let (token, title) = resolve_target(args.hwnd)?;
            // Required nonempty CLI URL overrides any compiled-in backend and
            // prevents the debug-only local-token fallback from being selected.
            std::env::set_var("PETAL_BACKEND_URL", &args.backend);
            let identity = format!(
                "p-wgcrace-{}{:x}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "system clock precedes epoch")?
                    .as_millis()
            );
            let admission = tokio::time::timeout(
                STEP_BUDGET,
                fetch_access_token(BackendTokenRequest {
                    room: &args.room,
                    identity: &identity,
                    display_name: Some("Native WGC race fixture"),
                    access_code: None,
                    can_publish: true,
                    can_subscribe: true,
                    can_publish_data: true,
                    hidden: false,
                }),
            )
            .await
            .map_err(|_| "backend admission timed out")?
            .map_err(|_| "backend admission failed (no credentials logged)")?;
            self.connection = Some(
                tokio::time::timeout(
                    STEP_BUDGET,
                    RoomConnection::connect(&admission.url, &admission.token),
                )
                .await
                .map_err(|_| "LiveKit connection timed out")?
                .map_err(|_| "LiveKit connection failed (no credentials logged)")?,
            );
            let connection = self.connection.as_ref().unwrap();
            connection.discard_compositor_events();
            let latest = Arc::new(Mutex::new(None::<BgraFrame>));
            let callback_latest = latest.clone();
            let (capture, status): (_, CaptureStatus) = tokio::task::spawn_blocking(move || {
                TargetCaptureSession::start(token, CaptureIndicatorMode::System, move |frame| {
                    *callback_latest.lock().unwrap() = Some(frame);
                })
            })
            .await
            .map_err(|_| "WGC setup worker failed")??;
            self.capture = Some(capture);
            let first = tokio::time::timeout(STEP_BUDGET, async {
                loop {
                    if let Some(frame) = latest.lock().unwrap().take() {
                        return Ok(frame);
                    }
                    if status.terminal_error().is_some() {
                        return Err("WGC terminated before first frame");
                    }
                    tokio::time::sleep(Duration::from_millis(33)).await;
                }
            })
            .await
            .map_err(|_| "WGC first frame timed out")??;
            self.a = Some(
                tokio::time::timeout(
                    STEP_BUDGET,
                    connection.publish_window_at_cadence(
                        first.width,
                        first.height,
                        ShareQuality::Full,
                        Some(token),
                        30,
                    ),
                )
                .await
                .map_err(|_| "initial publication timed out")?
                .map_err(|_| "initial publication failed")?,
            );
            tokio::time::timeout(
                STEP_BUDGET,
                connection.set_shared_window_info(
                    token,
                    title,
                    1.0,
                    None,
                    VideoColorProfile::SRGB_BT709_FULL,
                    SharedSourceKind::Window,
                    None,
                ),
            )
            .await
            .map_err(|_| "share metadata timed out")?;
            let mut sequence = 1;
            let mut ready = push(self.a.as_ref().unwrap(), first, sequence);
            if ready {
                event("INITIAL_PUBLISHED", self.a.as_ref().unwrap(), token)?;
            }
            let mut awaiting_frame = tokio::time::Instant::now();
            let mut replacement_ready = false;
            let mut retired = false;
            let mut tick = tokio::time::interval(Duration::from_millis(33));
            loop {
                tick.tick().await;
                if status.terminal_error().is_some() {
                    return Err("WGC capture terminated".into());
                }
                if let Some(frame) = latest.lock().unwrap().take() {
                    sequence += 1;
                    let track = self.b.as_ref().or(self.a.as_ref()).unwrap();
                    if push(track, frame, sequence) {
                        if !ready {
                            event("INITIAL_PUBLISHED", track, token)?;
                            ready = true;
                        } else if self.b.is_some() && !replacement_ready {
                            event("REPLACEMENT_PUBLISHED", track, token)?;
                            replacement_ready = true;
                        }
                    }
                }
                if (!ready || (self.b.is_some() && !replacement_ready))
                    && awaiting_frame.elapsed() > STEP_BUDGET
                {
                    return Err("publication accepted no real WGC frame within 20 seconds".into());
                }
                for command in control.commands()? {
                    match command.as_str() {
                        "publish-b" if ready && self.b.is_none() => {
                            let a = self.a.as_ref().unwrap();
                            self.b = Some(
                                tokio::time::timeout(
                                    STEP_BUDGET,
                                    connection.publish_window_at_cadence(
                                        a.width(),
                                        a.height(),
                                        ShareQuality::Full,
                                        Some(token),
                                        a.cadence_fps(),
                                    ),
                                )
                                .await
                                .map_err(|_| "replacement publication timed out")?
                                .map_err(|_| "replacement publication failed")?,
                            );
                            if self.b.as_ref().unwrap().sid() == a.sid() {
                                return Err("replacement SID must differ from initial SID".into());
                            }
                            // Never push another frame into A. Discard the pre-
                            // transition slot; B readiness needs a fresh WGC frame.
                            latest.lock().unwrap().take();
                            awaiting_frame = tokio::time::Instant::now();
                        }
                        "retire-a" if replacement_ready && !retired => {
                            let a = self.a.as_ref().unwrap();
                            tokio::time::timeout(STEP_BUDGET, a.unpublish())
                                .await
                                .map_err(|_| "old publication retirement timed out")?
                                .map_err(|_| "old publication retirement failed")?;
                            event("OLD_RETIRED", a, token)?;
                            self.a.take();
                            retired = true;
                        }
                        "stop" => return Ok(()),
                        _ => return Err("unknown or out-of-order control command".into()),
                    }
                }
            }
        }

        async fn cleanup(&mut self) -> Result<()> {
            let mut clean = true;
            if let Some(capture) = self.capture.take() {
                clean &= matches!(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        tokio::task::spawn_blocking(move || drop(capture))
                    )
                    .await,
                    Ok(Ok(()))
                );
            }
            for track in [self.a.take(), self.b.take()].into_iter().flatten() {
                clean &= matches!(
                    tokio::time::timeout(Duration::from_secs(5), track.unpublish()).await,
                    Ok(Ok(()))
                );
            }
            if let Some(connection) = self.connection.take() {
                clean &= matches!(
                    tokio::time::timeout(Duration::from_secs(5), connection.room().close()).await,
                    Ok(Ok(()))
                );
            }
            if clean {
                Ok(())
            } else {
                Err("owned media cleanup failed or timed out".into())
            }
        }
    }

    pub async fn run() -> Result<()> {
        let args = Args::parse()?;
        let mut fixture = Fixture {
            connection: None,
            capture: None,
            a: None,
            b: None,
        };
        let outcome = tokio::time::timeout(RUN_BUDGET, fixture.drive(args))
            .await
            .unwrap_or_else(|_| Err("five-minute fixture deadline expired".into()));
        let cleanup = fixture.cleanup().await;
        outcome.and(cleanup)
    }
}
