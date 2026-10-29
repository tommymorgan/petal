//! One private Camera portal session per granted camera permission attempt.
//! Dropping the connection also retires requests that have not returned yet.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};

use tokio_util::sync::CancellationToken;

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const CAMERA: &str = "org.freedesktop.portal.Camera";
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(120);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
type Properties = HashMap<String, OwnedValue>;

/// The neutral camera-loss explanation from ADR 0003: without an explicit
/// portal denial identifying permission, access loss is indistinguishable
/// from device loss and must never be reported as a specific one of them.
pub(super) const CAMERA_ACCESS_LOST: &str =
    "Camera access or device lost; turn the camera on again to retry";

pub(super) struct CameraAccess {
    /// True only when the portal answered status 0 for AccessCamera.
    pub granted: bool,
}

fn denied_response(status: u32) -> String {
    match status {
        // Camera maps both denied access and dialog cancellation to status 1.
        1 => "Camera permission denied or cancelled; the camera stays off".to_string(),
        2 => "Camera permission request failed in the desktop portal; the camera stays off".to_string(),
        _ => "The desktop portal returned an unknown camera permission outcome".to_string(),
    }
}

pub(super) struct CameraPortalSession {
    connection: Option<zbus::Connection>,
    request_serial: u64,
}

impl CameraPortalSession {
    pub async fn connect() -> Result<Self, String> {
        let connection = tokio::time::timeout(Duration::from_secs(10), zbus::Connection::session())
            .await
            .map_err(|_| "Camera portal connection timed out")?
            .map_err(|e| format!("Camera portal unavailable: {e}"))?;
        let sender = connection
            .unique_name()
            .ok_or("Portal connection has no bus identity")?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        Ok(Self {
            connection: Some(connection),
            request_serial: 0,
        })
    }

    async fn proxy(&self) -> Result<zbus::Proxy<'_>, String> {
        zbus::Proxy::new(
            self.connection.as_ref().ok_or("Portal session retired")?,
            DESTINATION,
            DESKTOP,
            CAMERA,
        )
        .await
        .map_err(|e| format!("Camera portal unavailable: {e}"))
    }

    fn options(&mut self) -> HashMap<&'static str, Value<'static>> {
        self.request_serial += 1;
        HashMap::from([(
            "handle_token",
            Value::from(format!("petal_{}", self.request_serial)),
        )])
    }

    async fn request<B: serde::Serialize + DynamicType>(
        &self,
        method: &str,
        body: &B,
        cancel: &CancellationToken,
    ) -> Result<Properties, String> {
        let connection = self.connection.as_ref().ok_or("Portal session retired")?;
        let sender = connection
            .unique_name()
            .ok_or("Portal connection has no bus identity")?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!(
            "/org/freedesktop/portal/desktop/request/{sender}/petal_{}",
            self.request_serial
        );
        let request = zbus::Proxy::new(
            connection,
            DESTINATION,
            path.as_str(),
            "org.freedesktop.portal.Request",
        )
        .await
        .map_err(|e| format!("Camera request unavailable: {e}"))?;
        // Subscribe before sending the method: an immediate denial can race its reply.
        let mut responses = request
            .receive_signal("Response")
            .await
            .map_err(|e| format!("Camera response unavailable: {e}"))?;
        let operation = async {
            let returned: OwnedObjectPath = self
                .proxy()
                .await?
                .call(method, body)
                .await
                .map_err(|e| format!("Camera portal {method} failed: {e}"))?;
            if returned.as_str() != path {
                return Err("Camera portal returned an unexpected request identity".to_string());
            }
            let message = responses.next().await.ok_or("Camera portal disconnected")?;
            let (status, values): (u32, Properties) = message
                .body()
                .deserialize()
                .map_err(|e| format!("Invalid Camera response: {e}"))?;
            match status {
                0 => Ok(values),
                status => Err(denied_response(status)),
            }
        };
        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("Camera permission cancelled; the camera stays off".to_string()),
            result = tokio::time::timeout(PERMISSION_TIMEOUT, operation) =>
                result.unwrap_or_else(|_| Err("Camera permission timed out; turn the camera on again to retry".to_string())),
        };
        if outcome.is_err() {
            let _ =
                tokio::time::timeout(CLOSE_TIMEOUT, request.call::<_, _, ()>("Close", &())).await;
        }
        outcome
    }

    /// AccessCamera through the real Camera v1 interface. The Camera portal
    /// has no CreateSession: permission is requested directly, devices come
    /// from the restricted remote, and OpenPipeWireRemote takes only options.
    pub async fn request_access(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<CameraAccess, String> {
        let options = self.options();
        self.request("AccessCamera", &(options,), cancel).await?;
        Ok(CameraAccess { granted: true })
    }

    /// One restricted PipeWire remote fd. The Camera portal permits one fd per
    /// call; the owner opens separate metadata and capture remotes from the
    /// same granted access.
    pub async fn remote(&self) -> Result<std::os::fd::OwnedFd, String> {
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let remote: zbus::zvariant::OwnedFd = self
            .proxy()
            .await?
            .call("OpenPipeWireRemote", &(options,))
            .await
            .map_err(|e| format!("Restricted PipeWire remote unavailable: {e}"))?;
        Ok(remote.into())
    }

    pub async fn close(&mut self) {
        // The Camera portal has no session object to Close; retiring our
        // private bus connection is the whole teardown.
        if let Some(connection) = self.connection.take() {
            let _ = connection.close().await;
        }
    }
}

impl Drop for CameraPortalSession {
    fn drop(&mut self) {
        // Also covers cancellation/panic before AccessCamera has replied. This
        // connection is private: closing it cannot retire another grant.
        if let Some(connection) = self.connection.take() {
            tokio::spawn(async move {
                let _ = connection.close().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct PrivateConfig(std::path::PathBuf);

    impl Drop for PrivateConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    struct PrivateBus {
        child: Child,
        _config: PrivateConfig,
    }

    impl PrivateBus {
        fn start() -> (Self, String) {
            // --session reads the host's standard service directories and can
            // auto-activate a real desktop portal even on a private bus. This
            // fixture permits only services explicitly installed by the test.
            const CONFIG: &[u8] = br#"<busconfig>
                <type>session</type>
                <listen>unix:tmpdir=/tmp</listen>
                <auth>EXTERNAL</auth>
                <policy context="default">
                    <allow send_destination="*" eavesdrop="true"/>
                    <allow eavesdrop="true"/>
                    <allow own="*"/>
                </policy>
            </busconfig>"#;
            static NEXT_BUS: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "petal-camera-private-bus-{}-{}.conf",
                std::process::id(),
                NEXT_BUS.fetch_add(1, Ordering::SeqCst),
            ));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .expect("private bus configuration file");
            let config = PrivateConfig(path);
            file.write_all(CONFIG).expect("private bus configuration");
            drop(file);
            let mut config_arg = std::ffi::OsString::from("--config-file=");
            config_arg.push(&config.0);
            let mut child = Command::new("dbus-daemon")
                .arg(config_arg)
                .args(["--nofork", "--nopidfile", "--print-address=1"])
                .env_remove("DBUS_SESSION_BUS_ADDRESS")
                .env_remove("DBUS_STARTER_ADDRESS")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("private D-Bus fixture requires the installed dbus-daemon");
            let stdout = child.stdout.take().expect("private bus address pipe");
            let mut bus = Self {
                child,
                _config: config,
            };
            let (send, receive) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut address = String::new();
                let result = BufReader::new(stdout)
                    .read_line(&mut address)
                    .map(|_| address.trim().to_string());
                let _ = send.send(result);
            });
            match receive.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(address)) if !address.is_empty() => (bus, address),
                outcome => {
                    let _ = bus.child.kill();
                    let status = bus.child.wait();
                    let mut stderr = String::new();
                    if let Some(mut pipe) = bus.child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr);
                    }
                    panic!(
                        "private camera bus startup failed: {outcome:?}; status={status:?}; stderr={stderr}"
                    );
                }
            }
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    struct NegativeResponsePortal {
        response: Arc<AtomicU32>,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Camera")]
    impl NegativeResponsePortal {
        async fn access_camera(
            &self,
            options: Properties,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let token = options
                .get("handle_token")
                .and_then(|value| <&str>::try_from(value).ok())
                .ok_or_else(|| zbus::fdo::Error::Failed("missing request token".to_string()))?;
            let sender = header
                .sender()
                .ok_or_else(|| zbus::fdo::Error::Failed("missing caller identity".to_string()))?
                .as_str()
                .trim_start_matches(':')
                .replace('.', "_");
            let path = OwnedObjectPath::try_from(format!(
                "/org/freedesktop/portal/desktop/request/{sender}/{token}"
            ))
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            // A real D-Bus Response is deliberately emitted before the method
            // reply. Production must subscribe first and interpret its status,
            // not mistake a returned request path for granted permission.
            connection
                .emit_signal(
                    None::<&str>,
                    &path,
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(self.response.load(Ordering::SeqCst), Properties::new()),
                )
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            Ok(path)
        }
    }

    struct PendingRequest {
        closed: Arc<tokio::sync::Notify>,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Request")]
    impl PendingRequest {
        fn close(&self) {
            self.closed.notify_one();
        }
    }

    struct PendingResponsePortal {
        requests: tokio::sync::mpsc::UnboundedSender<Arc<tokio::sync::Notify>>,
    }

    #[zbus::interface(name = "org.freedesktop.portal.Camera")]
    impl PendingResponsePortal {
        async fn access_camera(
            &self,
            options: Properties,
            #[zbus(header)] header: zbus::message::Header<'_>,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let token = options
                .get("handle_token")
                .and_then(|value| <&str>::try_from(value).ok())
                .ok_or_else(|| zbus::fdo::Error::Failed("missing request token".to_string()))?;
            let sender = header
                .sender()
                .ok_or_else(|| zbus::fdo::Error::Failed("missing caller identity".to_string()))?
                .as_str()
                .trim_start_matches(':')
                .replace('.', "_");
            let path = OwnedObjectPath::try_from(format!(
                "/org/freedesktop/portal/desktop/request/{sender}/{token}"
            ))
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            let closed = Arc::new(tokio::sync::Notify::new());
            connection
                .object_server()
                .at(path.clone(), PendingRequest { closed: closed.clone() })
                .await
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            self.requests
                .send(closed)
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
            Ok(path)
        }
    }

    #[tokio::test]
    async fn camera_permission_off_closes_its_real_request_without_retiring_independent_access() {
        // Wire/lifecycle proof only: no permission response is invented and no
        // actual desktop portal, application or media device is operated.
        let (_bus, address) = PrivateBus::start();
        let (requests, mut observed) = tokio::sync::mpsc::unbounded_channel();
        let server = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(DESTINATION)
            .unwrap()
            .serve_at(DESKTOP, PendingResponsePortal { requests })
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut publication = CameraPortalSession {
            connection: Some(zbus::connection::Builder::address(address.as_str()).unwrap().build().await.unwrap()),
            request_serial: 0,
        };
        let mut independent = CameraPortalSession {
            connection: Some(zbus::connection::Builder::address(address.as_str()).unwrap().build().await.unwrap()),
            request_serial: 0,
        };
        let state = crate::session::SessionState::default();
        let generation = state.begin_room_generation();
        state.set_camera_intent(true);
        let request = state.begin_camera_permission_request(&generation).unwrap();
        let independent_cancel = CancellationToken::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            let (publication_result, ()) = tokio::join!(
                publication.request_access(request.cancellation()),
                async {
                    let publication_closed = observed.recv().await.unwrap();
                    let (independent_result, ()) = tokio::join!(
                        independent.request_access(&independent_cancel),
                        async {
                            let independent_closed = observed.recv().await.unwrap();
                            state.set_camera_intent(false);
                            publication_closed.notified().await;
                            assert!(!independent_cancel.is_cancelled());
                            assert!(matches!(
                                independent_closed.notified().now_or_never(),
                                None
                            ));
                            independent_cancel.cancel();
                            independent_closed.notified().await;
                        },
                    );
                    assert!(independent_result.err().expect("independent cleanup cancels access").contains("cancelled"));
                },
            );
            assert!(publication_result.err().expect("Off cancels access").contains("cancelled"));
        })
        .await
        .expect("Off must close the owned request without a permission response");
        publication.close().await;
        independent.close().await;
        server.close().await.unwrap();
    }

    #[tokio::test]
    async fn negative_camera_responses_never_become_granted_access() {
        // Protocol/error-state test, not GNOME permission acceptance. Its
        // private bus is passed directly to the production session; global
        // environment and the actual desktop portal are never replaced.
        let (_bus, address) = PrivateBus::start();
        let response = Arc::new(AtomicU32::new(1));
        let server = zbus::connection::Builder::address(address.as_str())
            .expect("private server address")
            .name(DESTINATION)
            .expect("private portal name")
            .serve_at(
                DESKTOP,
                NegativeResponsePortal {
                    response: response.clone(),
                },
            )
            .expect("private response interface")
            .build()
            .await
            .expect("private response server");
        let connection = zbus::connection::Builder::address(address.as_str())
            .expect("private client address")
            .build()
            .await
            .expect("private response client");
        let mut portal = CameraPortalSession {
            connection: Some(connection),
            request_serial: 0,
        };
        for status in [1, 2, 255] {
            response.store(status, Ordering::SeqCst);
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                portal.request_access(&CancellationToken::new()),
            )
            .await
            .expect("production camera response handling must settle");
            assert!(result.is_err(), "a negative portal response must never grant camera access");
        }
        portal.close().await;
        server
            .close()
            .await
            .expect("private response server cleanup");
    }

    #[tokio::test]
    async fn unavailable_camera_portal_rejects_access_and_retires_private_connection() {
        // A real private bus with no portal service: no host service changes,
        // replacement portal, invented permission result or media device.
        let (_bus, address) = PrivateBus::start();
        let connection = zbus::connection::Builder::address(address.as_str())
            .expect("private client address")
            .build()
            .await
            .expect("private client");
        let bus = zbus::fdo::DBusProxy::new(&connection)
            .await
            .expect("private bus proxy");
        assert!(
            !bus.name_has_owner(DESTINATION.try_into().expect("portal bus name"))
                .await
                .expect("private portal owner query"),
            "Missing-portal fixture unexpectedly has a portal owner"
        );
        drop(bus);
        let sender = connection
            .unique_name()
            .expect("private client identity")
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let mut portal = CameraPortalSession {
            connection: Some(connection),
            request_serial: 0,
        };
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            portal.request_access(&CancellationToken::new()),
        )
        .await
        .expect("missing portal must settle without a permission timeout");
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("An unavailable portal must never grant camera access"),
        };
        assert!(
            error.contains("Camera portal") || error.contains("Camera request"),
            "Missing portal lost its operation context: {error}"
        );
        assert!(
            error.contains("ServiceUnknown") || error.contains("NameHasNoOwner"),
            "Missing portal must retain its real D-Bus service error: {error}"
        );
        portal.close().await;
        assert!(
            portal.connection.is_none(),
            "Failed attempt retained its private portal connection"
        );
        portal.close().await;
    }
}
