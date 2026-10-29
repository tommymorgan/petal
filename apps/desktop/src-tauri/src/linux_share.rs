//! Independent, picture-only Linux shares with private native consent.
//! Four active owners, one pending, one retiring and one coalesced intent.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use tauri::Emitter;
use tokio::sync::{oneshot, watch, Notify};
use tokio_util::sync::CancellationToken;

use crate::linux_share_capture::CapturePipeline;
use crate::linux_share_portal::PortalSession;
use crate::platform::cg::WindowFrame;
use crate::room_generation::RoomGeneration;
use crate::sync_ext::MutexExt;
use crate::transport::publisher::{PublishedTrack, RoomConnection, SharedSourceKind};
use crate::transport::ShareQuality;

const FRAME_TIMEOUT: Duration = Duration::from_secs(15);
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(20);

struct Slot {
    id: u32,
    cancel: CancellationToken,
    done: watch::Receiver<bool>,
    frame: Option<WindowFrame>,
    source_kind: SharedSourceKind,
    replace_id: Option<u32>,
}

// A queued explicit intent has no portal, capture or publication authority.
// Repeated selections replace this one value, not spawn more waiting workers.
struct SelectionIntent {
    id: u32,
    app: tauri::AppHandle,
    connection: Arc<RoomConnection>,
    generation: RoomGeneration,
    ready: oneshot::Sender<Result<(), String>>,
    replace_id: Option<u32>,
}

#[derive(Default)]
struct ShareSlots {
    active: Vec<Slot>,
    pending: Option<Slot>,
    retiring: Option<Slot>,
    selection: Option<SelectionIntent>,
    driver_running: bool,
}

#[derive(Default)]
pub(crate) struct PictureOwner {
    slots: Mutex<ShareSlots>,
    next_id: AtomicU32,
    metadata: Arc<tokio::sync::Mutex<()>>,
    selection_changed: Notify,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PictureShare {
    window_id: u32,
    title: String,
    source_kind: SharedSourceKind,
}

fn source_title(kind: SharedSourceKind) -> &'static str {
    match kind {
        SharedSourceKind::Display => "Selected monitor",
        _ => "Selected window",
    }
}

impl ShareSlots {
    fn admit(&self, replace_id: Option<u32>) -> Result<(), String> {
        if let Some(id) = replace_id {
            if !self
                .active
                .iter()
                .any(|s| s.id == id && !s.cancel.is_cancelled())
            {
                return Err("The selected picture share is no longer active".to_string());
            }
        } else if self.active.len() >= 4 {
            return Err(
                "You can share up to four monitors or windows; stop a share before adding another"
                    .to_string(),
            );
        }
        Ok(())
    }
}

impl PictureOwner {
    pub fn active_ids(&self) -> Vec<u32> {
        self.slots
            .lock_unpoisoned()
            .active
            .iter()
            .filter(|slot| !slot.cancel.is_cancelled())
            .map(|slot| slot.id)
            .collect()
    }

    pub fn summaries(&self) -> Vec<PictureShare> {
        self.slots
            .lock_unpoisoned()
            .active
            .iter()
            .filter(|slot| !slot.cancel.is_cancelled())
            .map(|slot| PictureShare {
                window_id: slot.id,
                title: source_title(slot.source_kind).to_string(),
                source_kind: slot.source_kind,
            })
            .collect()
    }

    pub fn source_kind(&self, id: u32) -> Option<SharedSourceKind> {
        self.slots
            .lock_unpoisoned()
            .active
            .iter()
            .find(|slot| slot.id == id && !slot.cancel.is_cancelled())
            .map(|s| s.source_kind)
    }

    pub fn frame(&self, id: u32) -> Option<WindowFrame> {
        self.slots
            .lock_unpoisoned()
            .active
            .iter()
            .find(|slot| slot.id == id && !slot.cancel.is_cancelled())
            .and_then(|s| s.frame)
    }

    fn update_geometry(&self, id: u32, generation: &RoomGeneration, frame: WindowFrame) {
        let mut slots = self.slots.lock_unpoisoned();
        if generation.is_current() {
            if let Some(slot) = slots
                .active
                .iter_mut()
                .find(|s| s.id == id && !s.cancel.is_cancelled())
            {
                slot.frame = Some(frame);
            }
        }
    }

    pub fn cancel(&self) {
        let mut slots = self.slots.lock_unpoisoned();
        if let Some(intent) = slots.selection.take() {
            let _ = intent
                .ready
                .send(Err("Screen sharing cancelled".to_string()));
        }
        for slot in slots
            .active
            .iter()
            .chain(slots.pending.iter())
            .chain(slots.retiring.iter())
        {
            slot.cancel.cancel();
        }
        self.selection_changed.notify_one();
    }

    fn is_current(&self, id: u32, generation: &RoomGeneration) -> bool {
        let slots = self.slots.lock_unpoisoned();
        generation.is_current()
            && slots
                .active
                .iter()
                .chain(slots.pending.iter())
                .any(|slot| slot.id == id && !slot.cancel.is_cancelled())
    }

    fn commit(
        &self,
        id: u32,
        generation: &RoomGeneration,
        frame: WindowFrame,
        source_kind: SharedSourceKind,
    ) -> Result<Option<watch::Receiver<bool>>, String> {
        let mut slots = self.slots.lock_unpoisoned();
        if !generation.is_current()
            || !slots
                .pending
                .as_ref()
                .is_some_and(|slot| slot.id == id && !slot.cancel.is_cancelled())
        {
            return Err("Screen sharing cancelled".to_string());
        }
        if slots.retiring.is_some() {
            return Err("The previous picture is still retiring".to_string());
        }
        let replace_id = slots.pending.as_ref().and_then(|s| s.replace_id);
        slots.admit(replace_id)?;
        let mut selected = slots.pending.take().ok_or("Picture selection retired")?;
        selected.frame = Some(frame);
        selected.source_kind = source_kind;
        // Only a source whose real frame was accepted may replace its target.
        let previous = replace_id.and_then(|target| {
            slots
                .active
                .iter()
                .position(|s| s.id == target)
                .map(|index| slots.active.remove(index))
        });
        slots.active.push(selected);
        let retired = previous.as_ref().map(|slot| {
            slot.cancel.cancel();
            slot.done.clone()
        });
        slots.retiring = previous;
        Ok(retired)
    }

    pub(crate) async fn stop(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let (stopped, cutoff) = {
            let mut slots = self.slots.lock_unpoisoned();
            if let Some(intent) = slots.selection.take() {
                let _ = intent
                    .ready
                    .send(Err("Screen sharing cancelled".to_string()));
            }
            // Keep each resource owner registered until its own cleanup ends.
            // A later explicit Start cannot bypass an outstanding Stop merely
            // because the IPC caller already requested cancellation.
            let stopped: Vec<_> = slots
                .active
                .iter()
                .chain(slots.pending.iter())
                .chain(slots.retiring.iter())
                .map(|slot| {
                    slot.cancel.cancel();
                    (slot.id, slot.done.clone())
                })
                .collect();
            (stopped, self.next_id.load(Ordering::SeqCst))
        };
        for (id, _) in &stopped {
            emit_state(app, *id, false, None);
        }
        emit_picker(app, cutoff, false);
        self.selection_changed.notify_one();
        tokio::time::timeout(PUBLISH_TIMEOUT, async {
            for (_, done) in stopped {
                wait_completion(Some(done)).await?;
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|_| {
            "Picture stop requested; waiting for the room service to retire its pending publication"
                .to_string()
        })??;
        Ok(())
    }

    fn cancel_one(
        &self,
        id: u32,
    ) -> Result<(watch::Receiver<bool>, Option<watch::Receiver<bool>>), String> {
        let mut slots = self.slots.lock_unpoisoned();
        let slot = slots
            .active
            .iter()
            .find(|s| s.id == id && !s.cancel.is_cancelled())
            .ok_or("The selected picture share is no longer active")?;
        slot.cancel.cancel();
        let done = slot.done.clone();
        let pending_done = slots
            .pending
            .as_ref()
            .filter(|s| s.replace_id == Some(id))
            .map(|pending| {
                pending.cancel.cancel();
                pending.done.clone()
            });
        if slots
            .selection
            .as_ref()
            .is_some_and(|s| s.replace_id == Some(id))
        {
            if let Some(intent) = slots.selection.take() {
                emit_picker(&intent.app, intent.id, false);
                let _ = intent
                    .ready
                    .send(Err("Replacement target stopped".to_string()));
            }
        }
        Ok((done, pending_done))
    }

    pub(crate) async fn stop_one(&self, app: &tauri::AppHandle, id: u32) -> Result<(), String> {
        let (done, pending) = self.cancel_one(id)?;
        emit_state(app, id, false, None);
        self.selection_changed.notify_one();
        tokio::time::timeout(PUBLISH_TIMEOUT, async {
            tokio::try_join!(wait_completion(Some(done)), wait_completion(pending))?;
            Ok::<_, String>(())
        })
        .await
        .map_err(|_| "Picture stop requested; waiting for publication retirement".to_string())?
    }

    async fn start(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        connection: Arc<RoomConnection>,
        generation: RoomGeneration,
        replace_id: Option<u32>,
    ) -> Result<(), String> {
        let (ready, result) = oneshot::channel();
        let (id, launch_driver) = {
            let mut slots = self.slots.lock_unpoisoned();
            if !generation.is_current() {
                return Err("Join a room before sharing a picture".to_string());
            }
            slots.admit(replace_id)?;
            let id = self
                .next_id
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
                .map_err(|_| "Picture share identities exhausted; restart Petal")?
                + 1;
            if let Some(pending) = &slots.pending {
                pending.cancel.cancel();
            }
            if let Some(previous) = slots.selection.replace(SelectionIntent {
                id,
                app: app.clone(),
                connection,
                generation,
                ready,
                replace_id,
            }) {
                let _ = previous
                    .ready
                    .send(Err("Picture selection superseded".to_string()));
            }
            let launch_driver = !slots.driver_running;
            slots.driver_running = true;
            (id, launch_driver)
        };
        emit_picker(&app, id, true);
        self.selection_changed.notify_one();
        if launch_driver {
            let owner = self.clone();
            tokio::spawn(async move {
                owner.drive_selection().await;
            });
        }
        result
            .await
            .map_err(|_| "Picture publication task ended unexpectedly".to_string())?
    }

    async fn drive_selection(self: Arc<Self>) {
        loop {
            let changed = self.selection_changed.notified();
            let launch = {
                let mut slots = self.slots.lock_unpoisoned();
                if slots.selection.is_none() {
                    slots.driver_running = false;
                    return;
                }
                if slots.pending.is_some()
                    || slots.retiring.is_some()
                    || slots.active.iter().any(|slot| slot.cancel.is_cancelled())
                {
                    None
                } else {
                    let intent = slots
                        .selection
                        .take()
                        .expect("selection checked under lock");
                    let cancel = CancellationToken::new();
                    let (done, completed) = watch::channel(false);
                    if let Err(error) = slots.admit(intent.replace_id) {
                        emit_picker(&intent.app, intent.id, false);
                        let _ = intent.ready.send(Err(error));
                        continue;
                    }
                    slots.pending = Some(Slot {
                        id: intent.id,
                        cancel: cancel.clone(),
                        done: completed,
                        frame: None,
                        source_kind: SharedSourceKind::Display,
                        replace_id: intent.replace_id,
                    });
                    Some((intent, cancel, done))
                }
            };
            if let Some((intent, cancel, done)) = launch {
                let owner = self.clone();
                // Only actual resource owners get a worker. A superseded
                // queued intent never creates a parked portal/publication job.
                tokio::spawn(async move {
                    owner
                        .run(
                            intent.app,
                            intent.connection,
                            intent.generation,
                            intent.id,
                            cancel,
                            intent.ready,
                        )
                        .await;
                    done.send_replace(true);
                });
            } else {
                changed.await;
            }
        }
    }

    // Called only after this owner's real resources and metadata are retired.
    // Keeping the identity-qualified transition together also makes late
    // completion ordering testable without opening any media authority.
    fn finish(&self, id: u32) -> (bool, bool) {
        let mut slots = self.slots.lock_unpoisoned();
        let active = slots.active.iter().any(|slot| slot.id == id);
        let pending = slots.pending.as_ref().is_some_and(|slot| slot.id == id);
        let retiring = slots.retiring.as_ref().is_some_and(|slot| slot.id == id);
        if active {
            slots.active.retain(|slot| slot.id != id);
            if let Some(replacement) = slots.pending.as_ref().filter(|s| s.replace_id == Some(id)) {
                replacement.cancel.cancel();
            }
            if slots
                .selection
                .as_ref()
                .is_some_and(|s| s.replace_id == Some(id))
            {
                if let Some(intent) = slots.selection.take() {
                    emit_picker(&intent.app, intent.id, false);
                    let _ = intent
                        .ready
                        .send(Err("Replacement target ended".to_string()));
                }
            }
        }
        if pending {
            slots.pending.take();
        }
        if retiring {
            slots.retiring.take();
        }
        (
            active || pending || retiring,
            pending || (active && slots.pending.is_none()),
        )
    }

    async fn run(
        self: Arc<Self>,
        app: tauri::AppHandle,
        connection: Arc<RoomConnection>,
        generation: RoomGeneration,
        id: u32,
        cancel: CancellationToken,
        ready: oneshot::Sender<Result<(), String>>,
    ) {
        let mut ready = Some(ready);
        let mut portal = None;
        let mut capture = None;
        let mut publication = PublicationLease(None);
        let mut publishing = None;
        let mut metadata_task = None;
        let mut metadata_written = false;
        let mut started = false;
        let outcome: Result<(), String> = async {
            if !self.is_current(id, &generation) {
                return Err("Screen sharing cancelled".to_string());
            }
            portal = Some(tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                result = PortalSession::connect() => result?,
            });
            let session = portal.as_mut().ok_or("ScreenCast session missing")?;
            let mut closed = session.closed().await?;
            let stream = tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = session.select_picture(&cancel) => result?,
            };
            let source_kind = stream.source_kind;
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let remote = tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                result = tokio::time::timeout(Duration::from_secs(10), session.remote()) =>
                    result.map_err(|_| "Restricted PipeWire remote timed out")??,
            };
            capture = Some(tokio::task::spawn_blocking(move ||
                CapturePipeline::open(remote, stream.node))
                .await.map_err(|e| format!("Native capture worker failed: {e}"))??);
            let source = capture.as_mut().ok_or("Native capture missing")?;
            let first = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = tokio::time::timeout(FRAME_TIMEOUT, source.next_frame()) =>
                    result.map_err(|_| "Selected picture produced no first frame; select Share to retry")??,
            };
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let mut geometry = WindowFrame { x: 0, y: 0, width: first.width as i32, height: first.height as i32 };
            let metadata_connection = connection.clone();
            let lock = self.metadata.clone();
            let scale = first.source_scale;
            let profile = first.color_profile;
            metadata_task = Some(tokio::spawn(async move {
                let _guard = lock.lock().await;
                metadata_connection.set_shared_window_info_for_generation(
                    id, id as u64, source_title(source_kind).to_string(), scale, None, profile, source_kind,
                ).await;
                metadata_connection.set_shared_remote_control_allowed(id, false).await;
            }));
            metadata_written = true;
            tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = metadata_task.as_mut().ok_or("Picture metadata task missing")? => {
                    metadata_task = None;
                    result.map_err(|e| format!("Picture metadata failed: {e}"))?;
                },
            }
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let publisher = connection.clone();
            let width = first.width;
            let height = first.height;
            publishing = Some(tokio::spawn(async move {
                publisher.publish_window_at(width, height, ShareQuality::Full, Some(id)).await
                    .map(|track| PublicationLease(Some(Arc::new(track))))
                    .map_err(|e| format!("Native picture publication rejected: {e}"))
            }));
            publication = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = tokio::time::timeout(PUBLISH_TIMEOUT, publishing.as_mut().ok_or("Picture publisher missing")?) => {
                    let result = result.map_err(|_| "Native picture publication timed out")?;
                    // A completed handle must not be awaited again by cleanup,
                    // including when its task or native publication rejected.
                    publishing = None;
                    result.map_err(|e| format!("Native picture publisher failed: {e}"))??
                },
            };
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let published = publication.0.as_ref().ok_or("Native picture track missing")?;
            let mut accepted = published.push_frame(&first, now_us())
                .ok_or("Native publisher could not convert the selected picture frame")?.source_accepted;
            drop(first);
            let first_push_deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
            while !accepted {
                let frame = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                    _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                    result = tokio::time::timeout_at(first_push_deadline, source.next_frame()) =>
                        result.map_err(|_| "Native publisher accepted no first picture frame")??,
                };
                if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
                let timing = published.push_frame(&frame, now_us())
                    .ok_or("Native publisher could not convert the selected picture frame")?;
                geometry.width = frame.width as i32;
                geometry.height = frame.height as i32;
                accepted = timing.source_accepted;
            }
            let mut retiring = self.commit(id, &generation, geometry, source_kind)?;
            started = true;
            emit_state(&app, id, true, None);
            if retiring.is_none() {
                emit_picker(&app, id, false);
                if let Some(ready) = ready.take() { let _ = ready.send(Ok(())); }
            }
            loop {
                let awaiting_retirement = retiring.is_some();
                let frame = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    result = wait_retirement(&mut retiring), if awaiting_retirement => {
                        result?;
                        retiring = None;
                        emit_picker(&app, id, false);
                        if let Some(ready) = ready.take() { let _ = ready.send(Ok(())); }
                        continue;
                    },
                    _ = closed.next() => return Err("Picture sharing access ended; select Share explicitly to retry".to_string()),
                    result = source.next_frame() => result?,
                };
                // Stop/leave can win between sample delivery and this push. The
                // frame guard and share-ID-qualified cleanup never affect a newer slot.
                if !self.is_current(id, &generation) { break; }
                // Encoder backpressure may drop a valid frame; it is not a
                // capture failure and must not retire an otherwise live share.
                if published.push_frame(&frame, now_us()).is_none() {
                    return Err("Native picture publisher could not convert the selected frame".to_string());
                }
                self.update_geometry(id, &generation, WindowFrame {
                    x: 0, y: 0, width: frame.width as i32, height: frame.height as i32,
                });
            }
            Ok(())
        }.await;
        cancel.cancel();
        // Release OS media authority before waiting on any delayed signaling.
        if let Some(capture) = capture.take() {
            let _ = tokio::task::spawn_blocking(move || drop(capture)).await;
        }
        if let Some(mut portal) = portal.take() {
            portal.close().await;
        }
        if let Some(task) = publishing.take() {
            // Never abort/drop a pending SDK publication: a late track is owned
            // here and immediately unpublished rather than leaking into the room.
            if let Ok(Ok(late)) = task.await {
                publication = late;
            }
        }
        publication.close().await;
        if let Some(task) = metadata_task.take() {
            let _ = task.await;
        }
        if metadata_written {
            let _guard = self.metadata.lock().await;
            connection
                .clear_shared_window_title_for_generation(id, id as u64)
                .await;
        }
        let (removed, close_picker) = self.finish(id);
        if removed {
            emit_state(
                &app,
                id,
                false,
                if started {
                    outcome.as_ref().err().map(String::as_str)
                } else {
                    None
                },
            );
            if close_picker {
                emit_picker(&app, id, false);
            }
        }
        self.selection_changed.notify_one();
        if let Some(ready) = ready {
            let _ = ready.send(outcome);
        }
    }
}

async fn wait_completion(done: Option<watch::Receiver<bool>>) -> Result<(), String> {
    if let Some(mut done) = done {
        wait_done(&mut done).await?;
    }
    Ok(())
}

async fn wait_done(done: &mut watch::Receiver<bool>) -> Result<(), String> {
    while !*done.borrow_and_update() {
        done.changed()
            .await
            .map_err(|_| "Picture cleanup task ended unexpectedly")?;
    }
    Ok(())
}

async fn wait_retirement(done: &mut Option<watch::Receiver<bool>>) -> Result<(), String> {
    match done {
        Some(done) => wait_done(done).await,
        None => std::future::pending().await,
    }
}

struct PublicationLease(Option<Arc<PublishedTrack>>);
impl PublicationLease {
    async fn close(&mut self) {
        if let Some(track) = self.0.take() {
            if let Err(error) = track.unpublish().await {
                log::warn!("linux picture unpublish failed: {error}");
            }
        }
    }
}
impl Drop for PublicationLease {
    fn drop(&mut self) {
        if let Some(track) = self.0.take() {
            tokio::spawn(async move {
                let _ = track.unpublish().await;
            });
        }
    }
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

fn emit_state(app: &tauri::AppHandle, id: u32, shared: bool, error: Option<&str>) {
    let _ = app.emit(
        "linux-share-state-changed",
        serde_json::json!({"windowId": id, "shared": shared, "error": error}),
    );
    let _ = app.emit("share-picker-changed", ());
}

fn emit_picker(app: &tauri::AppHandle, id: u32, open: bool) {
    let _ = app.emit(
        "linux-share-picker-open",
        serde_json::json!({"open": open, "attemptId": id}),
    );
}

#[tauri::command]
pub(crate) async fn toggle_window_picker_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
) -> Result<bool, String> {
    // Admission precedes any portal lookup or device permission request.
    let Some((connection, generation)) = state.picture_connection_snapshot() else {
        return Err("Join a room before sharing a picture".to_string());
    };
    let owner = &state.pictures;
    let occupied = {
        let slots = owner.slots.lock_unpoisoned();
        !slots.active.is_empty()
            || slots.pending.is_some()
            || slots.retiring.is_some()
            || slots.selection.is_some()
    };
    if occupied {
        owner.stop(&app).await?;
    } else {
        owner.start(app, connection, generation, None).await?;
    }
    Ok(false)
}

#[tauri::command]
pub(crate) async fn open_window_picker_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
    color: Option<String>,
    replace_window_id: Option<u32>,
) -> Result<(), String> {
    // Linux picture capture has no colored overlay. Selection always belongs
    // to a fresh, explicit native permission attempt, never the current stream.
    let _ = color;
    let Some((connection, generation)) = state.picture_connection_snapshot() else {
        return Err("Join a room before sharing a picture".to_string());
    };
    state
        .pictures
        .start(app, connection, generation, replace_window_id)
        .await
}

#[tauri::command]
pub(crate) fn shared_window_ids(state: tauri::State<'_, crate::session::SessionState>) -> Vec<u32> {
    state.pictures.active_ids()
}

#[tauri::command]
pub(crate) async fn stop_all_shares(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
) -> Result<(), String> {
    state.pictures.stop(&app).await
}

#[tauri::command]
pub(crate) fn linux_picture_shares(
    state: tauri::State<'_, crate::session::SessionState>,
) -> Vec<PictureShare> {
    state.pictures.summaries()
}

#[tauri::command]
pub(crate) async fn stop_linux_picture_share(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
    window_id: u32,
) -> Result<(), String> {
    state.pictures.stop_one(&app, window_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: u32, replace_id: Option<u32>) -> Slot {
        let (_, done) = watch::channel(false);
        Slot {
            id,
            cancel: CancellationToken::new(),
            done,
            frame: None,
            source_kind: SharedSourceKind::Display,
            replace_id,
        }
    }

    fn geometry(width: i32) -> WindowFrame {
        WindowFrame {
            x: 0,
            y: 0,
            width,
            height: 720,
        }
    }

    fn generation() -> RoomGeneration {
        RoomGeneration::new(Arc::new(std::sync::atomic::AtomicU64::new(7)), 7)
    }

    fn commit(owner: &PictureOwner, id: u32, replace: Option<u32>, kind: SharedSourceKind) {
        owner.slots.lock_unpoisoned().pending = Some(slot(id, replace));
        owner
            .commit(id, &generation(), geometry(1280), kind)
            .unwrap();
    }

    #[test]
    fn four_independent_owners_reject_fifth_without_evicting_and_retire_only_one() {
        let owner = PictureOwner::default();
        for id in 1..=4 {
            owner.slots.lock_unpoisoned().admit(None).unwrap();
            commit(&owner, id, None, SharedSourceKind::Window);
        }
        assert_eq!(owner.active_ids(), vec![1, 2, 3, 4]);
        assert!(owner
            .slots
            .lock_unpoisoned()
            .admit(None)
            .unwrap_err()
            .contains("four"));
        assert!(owner.slots.lock_unpoisoned().admit(Some(3)).is_ok());
        assert!(owner.slots.lock_unpoisoned().admit(Some(99)).is_err());
        owner.finish(2);
        assert_eq!(owner.active_ids(), vec![1, 3, 4]);
        assert!(owner.slots.lock_unpoisoned().admit(None).is_ok());
    }

    #[test]
    fn replacement_and_late_cleanup_are_targeted_and_geometry_is_current() {
        let owner = PictureOwner::default();
        let generation = generation();
        commit(&owner, 1, None, SharedSourceKind::Display);
        commit(&owner, 2, None, SharedSourceKind::Window);
        owner.slots.lock_unpoisoned().pending = Some(slot(3, Some(1)));
        assert!(owner.is_current(1, &generation));
        assert_eq!(owner.active_ids(), vec![1, 2]);
        owner
            .commit(3, &generation, geometry(960), SharedSourceKind::Window)
            .unwrap();
        assert!(!owner.is_current(1, &generation));
        assert!(owner.is_current(2, &generation));
        assert_eq!(owner.active_ids(), vec![2, 3]);
        assert_eq!(owner.finish(1), (true, false));
        owner.update_geometry(3, &generation, geometry(1920));
        owner.update_geometry(1, &generation, geometry(640));
        assert_eq!(owner.frame(3).unwrap().width, 1920);
        assert_eq!(owner.frame(2).unwrap().width, 1280);
        assert_eq!(owner.finish(1), (false, false));
        assert_eq!(owner.active_ids(), vec![2, 3]);
    }

    #[test]
    fn failed_pending_and_stale_replacement_never_change_existing_pictures() {
        let owner = PictureOwner::default();
        commit(&owner, 1, None, SharedSourceKind::Display);
        owner.slots.lock_unpoisoned().pending = Some(slot(2, Some(1)));
        owner.finish(2);
        assert_eq!(owner.active_ids(), vec![1]);
        owner.slots.lock_unpoisoned().pending = Some(slot(3, Some(1)));
        owner.finish(1);
        assert!(owner
            .commit(3, &generation(), geometry(960), SharedSourceKind::Window)
            .is_err());
        assert!(owner.active_ids().is_empty());
        assert!(owner.slots.lock_unpoisoned().pending.is_some());
    }

    #[test]
    fn room_retirement_and_stop_keep_registered_resources_until_cleanup() {
        let owner = PictureOwner::default();
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(7));
        let old = RoomGeneration::new(counter.clone(), 7);
        commit(&owner, 1, None, SharedSourceKind::Window);
        owner.slots.lock_unpoisoned().pending = Some(slot(2, None));
        counter.store(8, Ordering::SeqCst);
        assert!(!owner.is_current(1, &old));
        assert!(!owner.is_current(2, &old));
        assert!(owner
            .commit(2, &old, geometry(960), SharedSourceKind::Display)
            .is_err());
        owner.update_geometry(1, &old, geometry(42));
        assert_eq!(owner.frame(1).unwrap().width, 1280);
        owner.cancel();
        assert!(owner.active_ids().is_empty());
        {
            let slots = owner.slots.lock_unpoisoned();
            assert_eq!(slots.active.len(), 1);
            assert!(slots.active[0].cancel.is_cancelled());
            assert!(slots.pending.as_ref().unwrap().cancel.is_cancelled());
        }
        owner.finish(1);
        owner.finish(2);
        assert!(owner.slots.lock_unpoisoned().active.is_empty());
        assert!(owner.slots.lock_unpoisoned().pending.is_none());
    }

    #[test]
    fn individual_stop_retires_only_its_target_and_pending_replacement() {
        let owner = PictureOwner::default();
        commit(&owner, 1, None, SharedSourceKind::Display);
        commit(&owner, 2, None, SharedSourceKind::Window);
        owner.slots.lock_unpoisoned().pending = Some(slot(3, Some(1)));
        assert!(owner.cancel_one(99).is_err());
        assert_eq!(owner.active_ids(), vec![1, 2]);
        let (_, pending) = owner.cancel_one(1).unwrap();
        assert!(pending.is_some());
        assert_eq!(owner.active_ids(), vec![2]);
        {
            let slots = owner.slots.lock_unpoisoned();
            assert_eq!(
                slots.active.len(),
                2,
                "Stop must retain real resource owners until cleanup"
            );
            assert!(slots.pending.as_ref().unwrap().cancel.is_cancelled());
        }
        assert!(
            owner.cancel_one(1).is_err(),
            "Stale Stop must not become Stop all"
        );
        owner.finish(1);
        owner.finish(3);
        assert_eq!(owner.active_ids(), vec![2]);
        assert!(owner.is_current(2, &generation()));
    }
}
