//! Linux picture-only monitor publication through the existing room publisher.
//! Active, pending and retiring resource owners remain registered until cleanup.
//! One authority-free latest selection intent waits for those bounded owners;
//! replacing it cancels only pending selection, never the current live picture.

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
}

// A queued explicit intent has no portal, capture or publication authority.
// Repeated selections replace this one value, not spawn more waiting workers.
struct SelectionIntent {
    id: u32,
    app: tauri::AppHandle,
    connection: Arc<RoomConnection>,
    generation: RoomGeneration,
    ready: oneshot::Sender<Result<(), String>>,
}

#[derive(Default)]
struct ShareSlots {
    active: Option<Slot>,
    pending: Option<Slot>,
    retiring: Option<Slot>,
    selection: Option<SelectionIntent>,
    driver_running: bool,
}

#[derive(Default)]
pub(crate) struct MonitorOwner {
    slots: Mutex<ShareSlots>,
    next_id: AtomicU32,
    metadata: Arc<tokio::sync::Mutex<()>>,
    selection_changed: Notify,
}

impl MonitorOwner {
    pub fn active_ids(&self) -> Vec<u32> {
        self.slots
            .lock_unpoisoned()
            .active
            .as_ref()
            .filter(|slot| !slot.cancel.is_cancelled())
            .map(|slot| vec![slot.id])
            .unwrap_or_default()
    }

    pub fn frame(&self, id: u32) -> Option<WindowFrame> {
        self.slots
            .lock_unpoisoned()
            .active
            .as_ref()
            .filter(|slot| slot.id == id && !slot.cancel.is_cancelled())
            .and_then(|slot| slot.frame)
    }

    pub fn cancel(&self) {
        let mut slots = self.slots.lock_unpoisoned();
        if let Some(intent) = slots.selection.take() {
            let _ = intent
                .ready
                .send(Err("Screen sharing cancelled".to_string()));
        }
        for slot in [&slots.active, &slots.pending, &slots.retiring]
            .into_iter()
            .flatten()
        {
            slot.cancel.cancel();
        }
        self.selection_changed.notify_one();
    }

    fn is_current(&self, id: u32, generation: &RoomGeneration) -> bool {
        let slots = self.slots.lock_unpoisoned();
        generation.is_current()
            && [&slots.active, &slots.pending]
                .into_iter()
                .flatten()
                .any(|slot| slot.id == id && !slot.cancel.is_cancelled())
    }

    fn commit(
        &self,
        id: u32,
        generation: &RoomGeneration,
        frame: WindowFrame,
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
            return Err("The previous monitor is still retiring".to_string());
        }
        let mut selected = slots.pending.take().ok_or("Monitor selection retired")?;
        selected.frame = Some(frame);
        // This is the sole source-change commit: the new track has accepted a
        // real frame. Before this point the original owner keeps pumping.
        let previous = slots.active.replace(selected);
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
            let stopped = [&slots.active, &slots.pending, &slots.retiring].map(|slot| {
                slot.as_ref().map(|slot| {
                    slot.cancel.cancel();
                    (slot.id, slot.done.clone())
                })
            });
            (stopped, self.next_id.load(Ordering::SeqCst))
        };
        for (id, _) in stopped.iter().flatten() {
            emit_state(app, *id, false, None);
        }
        emit_picker(app, cutoff, false);
        self.selection_changed.notify_one();
        tokio::time::timeout(PUBLISH_TIMEOUT, async {
            let [active, pending, retiring] = stopped.map(|slot| slot.map(|(_, done)| done));
            tokio::try_join!(
                wait_completion(active),
                wait_completion(pending),
                wait_completion(retiring)
            )?;
            Ok::<_, String>(())
        })
        .await
        .map_err(|_| {
            "Monitor stop requested; waiting for the room service to retire its pending publication"
                .to_string()
        })??;
        Ok(())
    }

    async fn start(
        self: &Arc<Self>,
        app: tauri::AppHandle,
        connection: Arc<RoomConnection>,
        generation: RoomGeneration,
    ) -> Result<(), String> {
        let (ready, result) = oneshot::channel();
        let (id, launch_driver) = {
            let mut slots = self.slots.lock_unpoisoned();
            if !generation.is_current() {
                return Err("Join a room before sharing a monitor".to_string());
            }
            let id = self
                .next_id
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
                .map_err(|_| "Monitor share identities exhausted; restart Petal")?
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
            }) {
                let _ = previous
                    .ready
                    .send(Err("Monitor selection superseded".to_string()));
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
            .map_err(|_| "Monitor publication task ended unexpectedly".to_string())?
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
                    || slots
                        .active
                        .as_ref()
                        .is_some_and(|slot| slot.cancel.is_cancelled())
                {
                    None
                } else {
                    let intent = slots
                        .selection
                        .take()
                        .expect("selection checked under lock");
                    let cancel = CancellationToken::new();
                    let (done, completed) = watch::channel(false);
                    slots.pending = Some(Slot {
                        id: intent.id,
                        cancel: cancel.clone(),
                        done: completed,
                        frame: None,
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
        let active = slots.active.as_ref().is_some_and(|slot| slot.id == id);
        let pending = slots.pending.as_ref().is_some_and(|slot| slot.id == id);
        let retiring = slots.retiring.as_ref().is_some_and(|slot| slot.id == id);
        if active {
            slots.active.take();
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
                result = session.select_monitor(&cancel) => result?,
            };
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let remote = tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                result = tokio::time::timeout(Duration::from_secs(10), session.remote()) =>
                    result.map_err(|_| "Restricted PipeWire remote timed out")??,
            };
            capture = Some(tokio::task::spawn_blocking(move ||
                CapturePipeline::open(remote, stream.node, stream.logical_size))
                .await.map_err(|e| format!("Native capture worker failed: {e}"))??);
            let source = capture.as_mut().ok_or("Native capture missing")?;
            let first = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = tokio::time::timeout(FRAME_TIMEOUT, source.next_frame()) =>
                    result.map_err(|_| "Selected monitor produced no first frame; select Share to retry")??,
            };
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let frame = WindowFrame { x: 0, y: 0, width: first.width as i32, height: first.height as i32 };
            let metadata_connection = connection.clone();
            let lock = self.metadata.clone();
            let scale = first.source_scale;
            let profile = first.color_profile;
            metadata_task = Some(tokio::spawn(async move {
                let _guard = lock.lock().await;
                metadata_connection.set_shared_window_info_for_generation(
                    id, id as u64, "Selected monitor".to_string(), scale, None, profile, SharedSourceKind::Display,
                ).await;
                metadata_connection.set_shared_remote_control_allowed(id, false).await;
            }));
            metadata_written = true;
            tokio::select! {
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = metadata_task.as_mut().ok_or("Monitor metadata task missing")? => {
                    metadata_task = None;
                    result.map_err(|e| format!("Monitor metadata failed: {e}"))?;
                },
            }
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let publisher = connection.clone();
            let width = first.width;
            let height = first.height;
            publishing = Some(tokio::spawn(async move {
                publisher.publish_window_at(width, height, ShareQuality::Full, Some(id)).await
                    .map(|track| PublicationLease(Some(Arc::new(track))))
                    .map_err(|e| format!("Native monitor publication rejected: {e}"))
            }));
            publication = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                result = tokio::time::timeout(PUBLISH_TIMEOUT, publishing.as_mut().ok_or("Monitor publisher missing")?) => {
                    let result = result.map_err(|_| "Native monitor publication timed out")?;
                    // A completed handle must not be awaited again by cleanup,
                    // including when its task or native publication rejected.
                    publishing = None;
                    result.map_err(|e| format!("Native monitor publisher failed: {e}"))??
                },
            };
            if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
            let published = publication.0.as_ref().ok_or("Native monitor track missing")?;
            let mut accepted = published.push_frame(&first, now_us())
                .ok_or("Native publisher could not convert the selected monitor frame")?.source_accepted;
            drop(first);
            let first_push_deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
            while !accepted {
                let frame = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err("Screen sharing cancelled".to_string()),
                    _ = closed.next() => return Err("Screen sharing access ended".to_string()),
                    result = tokio::time::timeout_at(first_push_deadline, source.next_frame()) =>
                        result.map_err(|_| "Native publisher accepted no first monitor frame")??,
                };
                if !self.is_current(id, &generation) { return Err("Screen sharing cancelled".to_string()); }
                let timing = published.push_frame(&frame, now_us())
                    .ok_or("Native publisher could not convert the selected monitor frame")?;
                accepted = timing.source_accepted;
            }
            let mut retiring = self.commit(id, &generation, frame)?;
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
                    _ = closed.next() => return Err("Monitor sharing access ended; select Share explicitly to retry".to_string()),
                    result = source.next_frame() => result?,
                };
                // Stop/leave can win between sample delivery and this push. The
                // frame guard and share-ID-qualified cleanup never affect a newer slot.
                if !self.is_current(id, &generation) { break; }
                // Encoder backpressure may drop a valid frame; it is not a
                // capture failure and must not retire an otherwise live share.
                if published.push_frame(&frame, now_us()).is_none() {
                    return Err("Native monitor publisher could not convert the monitor frame".to_string());
                }
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
            .map_err(|_| "Monitor cleanup task ended unexpectedly")?;
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
                log::warn!("linux monitor unpublish failed: {error}");
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
        "share-state-changed",
        serde_json::json!({"windowId": id, "shared": shared, "error": error}),
    );
    let _ = app.emit("share-picker-changed", ());
}

fn emit_picker(app: &tauri::AppHandle, id: u32, open: bool) {
    let _ = app.emit(
        "share-picker-visibility-changed",
        serde_json::json!({"open": open, "attemptId": id}),
    );
}

#[tauri::command]
pub(crate) async fn toggle_window_picker_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
) -> Result<bool, String> {
    // Admission precedes any portal lookup or device permission request.
    let Some((connection, generation)) = state.monitor_connection_snapshot() else {
        return Err("Join a room before sharing a monitor".to_string());
    };
    let owner = &state.monitor;
    let occupied = {
        let slots = owner.slots.lock_unpoisoned();
        slots.active.is_some()
            || slots.pending.is_some()
            || slots.retiring.is_some()
            || slots.selection.is_some()
    };
    if occupied {
        owner.stop(&app).await?;
    } else {
        owner.start(app, connection, generation).await?;
    }
    Ok(false)
}

#[tauri::command]
pub(crate) async fn open_window_picker_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
    color: Option<String>,
) -> Result<(), String> {
    // Linux picture capture has no colored overlay. Selection always belongs
    // to a fresh, explicit native permission attempt, never the current stream.
    let _ = color;
    let Some((connection, generation)) = state.monitor_connection_snapshot() else {
        return Err("Join a room before sharing a monitor".to_string());
    };
    state.monitor.start(app, connection, generation).await
}

#[tauri::command]
pub(crate) fn shared_window_ids(state: tauri::State<'_, crate::session::SessionState>) -> Vec<u32> {
    state.monitor.active_ids()
}

#[tauri::command]
pub(crate) async fn stop_all_shares(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::session::SessionState>,
) -> Result<(), String> {
    state.monitor.stop(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: u32) -> Slot {
        let (_, done) = watch::channel(false);
        Slot {
            id,
            cancel: CancellationToken::new(),
            done,
            frame: None,
        }
    }

    #[test]
    fn late_admission_frame_and_cleanup_preserve_committed_successor() {
        let owner = MonitorOwner::default();
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(7));
        let generation = RoomGeneration::new(counter, 7);
        // Only pending ownership is arranged here. Every activation and
        // retirement below uses the production transition/guard methods.
        // WindowFrame is geometry, not a fabricated captured/video frame.
        let a_geometry = WindowFrame {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        };
        let b_geometry = WindowFrame {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
        owner.slots.lock_unpoisoned().pending = Some(slot(10));
        assert!(owner.is_current(10, &generation));
        assert!(owner.commit(10, &generation, a_geometry).unwrap().is_none());
        owner.slots.lock_unpoisoned().pending = Some(slot(20));
        assert!(
            owner.is_current(10, &generation),
            "A must remain live while B is pending"
        );
        assert_eq!(owner.frame(10).unwrap().width, 640);
        assert!(owner.commit(20, &generation, b_geometry).unwrap().is_some());

        // This is the guard used immediately before handing ongoing samples
        // to the publisher. A delayed old sample must fail it after B commits.
        assert!(
            !owner.is_current(10, &generation),
            "Retired A admitted a late frame"
        );
        assert!(owner.is_current(20, &generation));
        let completed = owner.finish(10);
        assert_eq!(
            owner.active_ids(),
            vec![20],
            "A cleanup removed committed B"
        );
        assert_eq!(completed, (true, false));
        assert_eq!(owner.frame(20).unwrap().width, 1280);
        assert_eq!(owner.frame(20).unwrap().height, 720);

        // Even with another valid pending owner, a late A admission must not
        // consume that pending slot or replace B with A's old geometry.
        owner.slots.lock_unpoisoned().pending = Some(slot(30));
        assert_eq!(
            owner.commit(10, &generation, a_geometry).unwrap_err(),
            "Screen sharing cancelled"
        );
        assert_eq!(owner.active_ids(), vec![20]);
        assert_eq!(owner.frame(20).unwrap().width, 1280);
        assert_eq!(owner.finish(10), (false, false));
        assert_eq!(
            owner
                .slots
                .lock_unpoisoned()
                .pending
                .as_ref()
                .map(|slot| slot.id),
            Some(30)
        );
    }

    #[test]
    fn cancellation_retains_resource_ownership_until_each_completion() {
        let owner = MonitorOwner::default();
        {
            let mut slots = owner.slots.lock_unpoisoned();
            slots.active = Some(slot(1));
            slots.pending = Some(slot(2));
            slots.retiring = Some(slot(3));
        }
        owner.cancel();
        assert!(owner.active_ids().is_empty());
        {
            let slots = owner.slots.lock_unpoisoned();
            for slot in [&slots.active, &slots.pending, &slots.retiring] {
                assert!(slot.as_ref().unwrap().cancel.is_cancelled());
            }
        }
        assert_eq!(owner.finish(2), (true, true));
        {
            let slots = owner.slots.lock_unpoisoned();
            assert_eq!(slots.active.as_ref().map(|slot| slot.id), Some(1));
            assert_eq!(slots.retiring.as_ref().map(|slot| slot.id), Some(3));
            assert!(slots.pending.is_none());
        }
        owner.finish(1);
        owner.finish(3);
        let slots = owner.slots.lock_unpoisoned();
        assert!(slots.active.is_none() && slots.pending.is_none() && slots.retiring.is_none());
    }

    #[test]
    fn room_retirement_rejects_pending_admission_and_old_frame_generation() {
        let owner = MonitorOwner::default();
        let counter = Arc::new(std::sync::atomic::AtomicU64::new(7));
        let old = RoomGeneration::new(counter.clone(), 7);
        let geometry = WindowFrame {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        };
        owner.slots.lock_unpoisoned().pending = Some(slot(10));
        assert!(owner.commit(10, &old, geometry).unwrap().is_none());
        owner.slots.lock_unpoisoned().pending = Some(slot(20));

        counter.store(8, Ordering::SeqCst);
        assert!(
            !owner.is_current(10, &old),
            "Old room admitted an ongoing frame"
        );
        assert!(
            !owner.is_current(20, &old),
            "Old room admitted a pending owner"
        );
        assert_eq!(
            owner.commit(20, &old, geometry).unwrap_err(),
            "Screen sharing cancelled"
        );
        assert_eq!(owner.active_ids(), vec![10]);
        assert_eq!(
            owner
                .slots
                .lock_unpoisoned()
                .pending
                .as_ref()
                .map(|slot| slot.id),
            Some(20)
        );
        owner.cancel();
        owner.finish(10);
        owner.finish(20);

        let current = RoomGeneration::new(counter, 8);
        owner.slots.lock_unpoisoned().pending = Some(slot(30));
        assert!(owner.commit(30, &current, geometry).unwrap().is_none());
        assert_eq!(owner.finish(10), (false, false));
        assert_eq!(owner.finish(20), (false, false));
        assert!(owner.is_current(30, &current));
        assert!(!owner.is_current(30, &old));
        assert_eq!(owner.active_ids(), vec![30]);
    }
}
