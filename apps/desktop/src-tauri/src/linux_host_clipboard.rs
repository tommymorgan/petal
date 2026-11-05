//! Clipboard v1 bound to the same prepared RemoteDesktop session.
//! No content is read or advertised during preparation.
use crate::sync_ext::MutexExt;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Clipboard";
const MIME: &str = "text/plain;charset=utf-8";
pub(crate) struct PortalClipboard {
    connection: zbus::Connection,
    session: OwnedObjectPath,
    cancel: CancellationToken,
    sequence: AtomicU64,
    text: Mutex<Option<Arc<Vec<u8>>>>,
    formats: Mutex<Vec<String>>,
}
impl PortalClipboard {
    pub async fn open(
        connection: zbus::Connection,
        path: String,
        cancel: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        let session = OwnedObjectPath::try_from(path).map_err(|e| e.to_string())?;
        let owner = Arc::new(Self {
            connection,
            session,
            cancel,
            sequence: AtomicU64::new(0),
            text: Mutex::new(None),
            formats: Mutex::new(Vec::new()),
        });
        // Register both subscriptions before any SetSelection/read operation.
        let proxy = owner.proxy().await?;
        let mut transfers = proxy
            .receive_signal("SelectionTransfer")
            .await
            .map_err(|e| e.to_string())?;
        let mut changes = proxy
            .receive_signal("SelectionOwnerChanged")
            .await
            .map_err(|e| e.to_string())?;
        let worker = owner.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = worker.cancel.cancelled() => break,
                    change = changes.next() => {
                        let Some(change) = change else { break; };
                        if let Ok((session, values)) = change.body().deserialize::<(OwnedObjectPath, HashMap<String, OwnedValue>)>() {
                            if session == worker.session {
                                *worker.formats.lock_unpoisoned() = values.get("mime_types")
                                    .and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<String>::try_from(v).ok()).unwrap_or_default();
                                worker.sequence.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                    transfer = transfers.next() => {
                        let Some(transfer) = transfer else { break; };
                        let Ok((session, mime, serial)) = transfer.body().deserialize::<(OwnedObjectPath, String, u32)>() else { continue; };
                        if session != worker.session { continue; }
                        let text = worker.text.lock_unpoisoned().clone();
                        let success = if matches!(mime.as_str(), MIME | "text/plain") && !worker.cancel.is_cancelled() {
                            if let Some(text) = text { worker.transfer(serial, text).await.is_ok() } else { false }
                        } else { false };
                        if let Ok(proxy) = worker.proxy().await {
                            let _ = tokio::time::timeout(Duration::from_secs(2), proxy.call::<_, _, ()>("SelectionWriteDone", &(&worker.session, serial, success))).await;
                        }
                    }
                }
            }
            worker.text.lock_unpoisoned().take();
        });
        drop(proxy);
        Ok(owner)
    }
    async fn proxy(&self) -> Result<zbus::Proxy<'_>, String> {
        zbus::Proxy::new(&self.connection, DEST, PATH, INTERFACE)
            .await
            .map_err(|e| e.to_string())
    }
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }
    pub async fn read(&self) -> Result<Vec<u8>, String> {
        if self.cancel.is_cancelled() {
            return Err("Desktop clipboard session retired".into());
        }
        {
            let formats = self.formats.lock_unpoisoned();
            if formats.iter().any(|mime| {
                matches!(
                    mime.as_str(),
                    "text/uri-list"
                        | "application/vnd.portal.filetransfer"
                        | "x-special/gnome-copied-files"
                )
            }) || !formats
                .iter()
                .any(|mime| mime == MIME || mime == "text/plain")
            {
                return Err("Clipboard does not contain transferable plain text".into());
            }
        }
        let proxy = self.proxy().await?;
        let fd: zbus::zvariant::OwnedFd = tokio::time::timeout(
            Duration::from_secs(3),
            proxy.call("SelectionRead", &(&self.session, MIME)),
        )
        .await
        .map_err(|_| "Clipboard read timed out")?
        .map_err(|e| format!("Plain UTF-8 clipboard unavailable: {e}"))?;
        let cancel = self.cancel.clone();
        tokio::task::spawn_blocking(move || bounded_io(fd.into(), None, &cancel))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
        crate::remote_clipboard::validate_remote_text(&bytes).map_err(|e| e.to_string())?;
        if self.cancel.is_cancelled() {
            return Err("Desktop clipboard session retired".into());
        }
        let previous = self.text.lock_unpoisoned().replace(Arc::new(bytes));
        let options = HashMap::from([("mime_types", Value::from(vec![MIME, "text/plain"]))]);
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            self.proxy()
                .await?
                .call::<_, _, ()>("SetSelection", &(&self.session, options)),
        )
        .await;
        match result {
            Ok(Ok(())) => {
                self.sequence.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            _ => {
                *self.text.lock_unpoisoned() = previous;
                Err("Desktop clipboard ownership request failed".into())
            }
        }
    }
    async fn transfer(&self, serial: u32, text: Arc<Vec<u8>>) -> Result<(), String> {
        if self.cancel.is_cancelled() {
            return Err("Desktop clipboard retired".into());
        }
        let fd: zbus::zvariant::OwnedFd = self
            .proxy()
            .await?
            .call("SelectionWrite", &(&self.session, serial))
            .await
            .map_err(|e| e.to_string())?;
        let cancel = self.cancel.clone();
        tokio::task::spawn_blocking(move || bounded_io(fd.into(), Some(text), &cancel))
            .await
            .map_err(|e| e.to_string())??;
        Ok(())
    }
}
fn bounded_io(
    fd: std::os::fd::OwnedFd,
    output: Option<Arc<Vec<u8>>>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
    use std::os::fd::AsRawFd;
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err("Cannot configure clipboard descriptor".into());
    }
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    let mut offset = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
            return Err("Clipboard transfer expired".into());
        }
        if let Some(output) = &output {
            match file.write(&output[offset..]) {
                Ok(0) if offset < output.len() => return Err("Clipboard transfer closed".into()),
                Ok(n) => {
                    offset += n;
                    if offset == output.len() {
                        return Ok(Vec::new());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.to_string()),
            }
        } else {
            let mut buffer = [0u8; 8192];
            match file.read(&mut buffer) {
                Ok(0) => {
                    crate::remote_clipboard::validate_remote_text(&bytes)
                        .map_err(|e| e.to_string())?;
                    return Ok(bytes);
                }
                Ok(n) => {
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.len() > 1024 * 1024 {
                        return Err("Clipboard exceeds 1 MiB".into());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        let mut poll = libc::pollfd {
            fd: file.as_raw_fd(),
            events: if output.is_some() {
                libc::POLLOUT
            } else {
                libc::POLLIN
            },
            revents: 0,
        };
        unsafe {
            libc::poll(&mut poll, 1, 10);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read_fixture(bytes: Vec<u8>, cancel: CancellationToken) -> Result<Vec<u8>, String> {
        use std::os::unix::net::UnixStream;
        let (read, mut write) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || {
            let _ = write.write_all(&bytes);
        });
        let result = bounded_io(read.into(), None, &cancel);
        let _ = writer.join();
        result
    }
    #[test]
    fn portal_text_transfer_accepts_exact_limit_and_rejects_oversize_nul_and_retirement() {
        let exact = vec![b'a'; 1024 * 1024];
        assert_eq!(
            read_fixture(exact.clone(), CancellationToken::new()).unwrap(),
            exact
        );
        assert!(read_fixture(vec![b'a'; 1024 * 1024 + 1], CancellationToken::new()).is_err());
        assert!(read_fixture(b"no\0text".to_vec(), CancellationToken::new()).is_err());
        assert!(read_fixture(vec![0xff], CancellationToken::new()).is_err());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(read_fixture(b"retired".to_vec(), cancel).is_err());
    }
}
