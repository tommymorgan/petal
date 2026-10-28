//! One private portal connection/session per explicitly selected monitor.
//! Dropping the connection also retires requests that have not returned yet.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(120);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
type Properties = HashMap<String, OwnedValue>;

pub(super) struct MonitorStream {
    pub node: u32,
    pub logical_size: Option<(i32, i32)>,
}

pub(super) struct PortalSession {
    connection: Option<zbus::Connection>,
    path: String,
    request_serial: u64,
}

impl PortalSession {
    pub async fn connect() -> Result<Self, String> {
        let connection = tokio::time::timeout(Duration::from_secs(10), zbus::Connection::session())
            .await
            .map_err(|_| "ScreenCast portal connection timed out")?
            .map_err(|e| format!("ScreenCast portal unavailable: {e}"))?;
        let sender = connection
            .unique_name()
            .ok_or("Portal connection has no bus identity")?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        Ok(Self {
            connection: Some(connection),
            path: format!("/org/freedesktop/portal/desktop/session/{sender}/petal_monitor"),
            request_serial: 0,
        })
    }

    async fn proxy(&self) -> Result<zbus::Proxy<'_>, String> {
        zbus::Proxy::new(
            self.connection.as_ref().ok_or("Portal session retired")?,
            DESTINATION,
            DESKTOP,
            SCREENCAST,
        )
        .await
        .map_err(|e| format!("ScreenCast portal unavailable: {e}"))
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
        .map_err(|e| format!("ScreenCast request unavailable: {e}"))?;
        // Subscribe before sending the method: an immediate denial can race its reply.
        let mut responses = request
            .receive_signal("Response")
            .await
            .map_err(|e| format!("ScreenCast response unavailable: {e}"))?;
        let operation = async {
            let returned: OwnedObjectPath = self
                .proxy()
                .await?
                .call(method, body)
                .await
                .map_err(|e| format!("ScreenCast {method} failed: {e}"))?;
            if returned.as_str() != path {
                return Err("ScreenCast returned an unexpected request identity".to_string());
            }
            let message = responses
                .next()
                .await
                .ok_or("ScreenCast portal disconnected")?;
            let (status, values): (u32, Properties) = message
                .body()
                .deserialize()
                .map_err(|e| format!("Invalid ScreenCast response: {e}"))?;
            match status {
                0 => Ok(values),
                1 => Err("Screen sharing cancelled; no new content selected".to_string()),
                2 => Err(
                    "Screen sharing permission denied or the portal could not start capture"
                        .to_string(),
                ),
                _ => Err("ScreenCast returned an unknown permission outcome".to_string()),
            }
        };
        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("Screen sharing cancelled".to_string()),
            result = tokio::time::timeout(PERMISSION_TIMEOUT, operation) =>
                result.unwrap_or_else(|_| Err("Screen sharing permission timed out; select Share to retry".to_string())),
        };
        if outcome.is_err() {
            let _ =
                tokio::time::timeout(CLOSE_TIMEOUT, request.call::<_, _, ()>("Close", &())).await;
        }
        outcome
    }

    pub async fn select_monitor(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<MonitorStream, String> {
        let mut options = self.options();
        options.insert("session_handle_token", Value::from("petal_monitor"));
        let mut created = self.request("CreateSession", &(options,), cancel).await?;
        // Some portal backends encode this as a string rather than an object path.
        let handle = created
            .remove("session_handle")
            .ok_or("ScreenCast returned no session")?;
        let actual = if let Ok(path) = <&str>::try_from(&handle) {
            path.to_string()
        } else {
            OwnedObjectPath::try_from(handle)
                .map_err(|_| "Invalid ScreenCast session identity")?
                .to_string()
        };
        if actual != self.path {
            return Err("ScreenCast returned an unexpected session identity".to_string());
        }
        let proxy = self.proxy().await?;
        let sources: u32 = proxy
            .get_property("AvailableSourceTypes")
            .await
            .map_err(|e| format!("ScreenCast monitor support unavailable: {e}"))?;
        let cursors: u32 = proxy
            .get_property("AvailableCursorModes")
            .await
            .map_err(|e| format!("ScreenCast cursor support unavailable: {e}"))?;
        if sources & 1 == 0 || cursors & 2 == 0 {
            return Err(
                "ScreenCast portal needs monitor capture with an embedded cursor".to_string(),
            );
        }
        drop(proxy);
        let session = OwnedObjectPath::try_from(self.path.clone()).map_err(|e| e.to_string())?;
        let mut options = self.options();
        options.insert("types", Value::from(1u32)); // Monitor only; never request a window/input device.
        options.insert("multiple", Value::from(false));
        options.insert("cursor_mode", Value::from(2u32)); // Embedded in the selected picture.
        options.insert("persist_mode", Value::from(0u32)); // No restore token or silent reacquisition.
        self.request("SelectSources", &(&session, options), cancel)
            .await?;
        let options = self.options();
        let mut response = self
            .request("Start", &(&session, "", options), cancel)
            .await?;
        let streams = response
            .remove("streams")
            .ok_or("ScreenCast returned no monitor stream")?;
        let mut streams: Vec<(u32, Properties)> = streams
            .try_into()
            .map_err(|e| format!("Invalid ScreenCast streams: {e}"))?;
        if streams.len() != 1 {
            return Err("Select exactly one monitor for this share".to_string());
        }
        let (node, properties) = streams
            .pop()
            .ok_or("ScreenCast returned no monitor stream")?;
        if node == 0
            || properties
                .get("source_type")
                .and_then(|v| u32::try_from(v).ok())
                != Some(1)
        {
            return Err("ScreenCast did not return the explicitly selected monitor".to_string());
        }
        let logical_size = properties
            .get("size")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| <(i32, i32)>::try_from(v).ok());
        Ok(MonitorStream { node, logical_size })
    }

    pub async fn remote(&self) -> Result<std::os::fd::OwnedFd, String> {
        let session = OwnedObjectPath::try_from(self.path.clone()).map_err(|e| e.to_string())?;
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let remote: zbus::zvariant::OwnedFd = self
            .proxy()
            .await?
            .call("OpenPipeWireRemote", &(&session, options))
            .await
            .map_err(|e| format!("Restricted PipeWire remote unavailable: {e}"))?;
        Ok(remote.into())
    }

    pub async fn closed(&self) -> Result<zbus::proxy::SignalStream<'static>, String> {
        let proxy = zbus::Proxy::new(
            self.connection.as_ref().ok_or("Portal session retired")?,
            DESTINATION,
            self.path.as_str(),
            "org.freedesktop.portal.Session",
        )
        .await
        .map_err(|e| format!("ScreenCast lifetime unavailable: {e}"))?;
        proxy
            .receive_signal("Closed")
            .await
            .map_err(|e| format!("ScreenCast lifetime unavailable: {e}"))
    }

    pub async fn close(&mut self) {
        if let Some(connection) = self.connection.take() {
            if let Ok(proxy) = zbus::Proxy::new(
                &connection,
                DESTINATION,
                self.path.as_str(),
                "org.freedesktop.portal.Session",
            )
            .await
            {
                let _ =
                    tokio::time::timeout(CLOSE_TIMEOUT, proxy.call::<_, _, ()>("Close", &())).await;
            }
            let _ = connection.close().await;
        }
    }
}

impl Drop for PortalSession {
    fn drop(&mut self) {
        // Also covers cancellation/panic before CreateSession has replied. This
        // connection is private: closing it cannot retire another share's grant.
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
                "petal-private-bus-{}-{}.conf",
                std::process::id(),
                NEXT_BUS.fetch_add(1, Ordering::SeqCst),
            ));
            // D-Bus reads configuration as a sized regular file. A pipe at
            // /dev/stdin does not provide that contract, even with valid XML.
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
                    panic!("private bus startup failed: {outcome:?}; status={status:?}; stderr={stderr}");
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

    #[zbus::interface(name = "org.freedesktop.portal.ScreenCast")]
    impl NegativeResponsePortal {
        async fn start(
            &self,
            _session: OwnedObjectPath,
            _parent: &str,
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
            // not mistake a returned request path for successful permission.
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

    #[tokio::test]
    async fn denied_and_cancelled_portal_responses_never_become_selected_content() {
        // This is a protocol/error-state test, not GNOME permission acceptance.
        // Its private bus is passed directly to the production session; global
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
        let sender = connection
            .unique_name()
            .expect("private client identity")
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let mut portal = PortalSession {
            connection: Some(connection),
            path: format!("/org/freedesktop/portal/desktop/session/{sender}/petal_monitor"),
            request_serial: 0,
        };
        for (status, expected) in [
            (1, "cancelled"),
            (2, "permission denied"),
            (255, "unknown permission outcome"),
        ] {
            response.store(status, Ordering::SeqCst);
            let options = portal.options();
            let session =
                OwnedObjectPath::try_from(portal.path.clone()).expect("private session path");
            let cancel = CancellationToken::new();
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                portal.request("Start", &(&session, "", options), &cancel),
            )
            .await
            .expect("production response handling must settle");
            let error = result.expect_err("a negative portal response must never select content");
            assert!(
                error.to_lowercase().contains(expected),
                "portal status {status} must retain its real error state, got {error}"
            );
        }
        portal.close().await;
        server
            .close()
            .await
            .expect("private response server cleanup");
    }

    #[tokio::test]
    async fn unavailable_portal_rejects_selection_and_retires_private_connection() {
        // A real private bus with no portal service: no host service changes,
        // replacement portal, invented permission result or media device.
        // This covers the protocol/error layer, not live DP-1 preservation.
        let (_bus, address) = PrivateBus::start();
        let connection = zbus::connection::Builder::address(address.as_str())
            .expect("private client address")
            .build()
            .await
            .expect("private client");
        let bus = zbus::fdo::DBusProxy::new(&connection)
            .await
            .expect("private bus proxy");
        let activatable = bus
            .list_activatable_names()
            .await
            .expect("private activation names");
        assert!(
            !activatable.iter().any(|name| name.as_str() == DESTINATION),
            "Missing-portal fixture must not auto-activate the host desktop portal"
        );
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
        let mut portal = PortalSession {
            connection: Some(connection),
            path: format!("/org/freedesktop/portal/desktop/session/{sender}/petal_monitor"),
            request_serial: 0,
        };
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            portal.select_monitor(&CancellationToken::new()),
        )
        .await
        .expect("missing portal must settle without a permission timeout");
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("An unavailable portal must never select monitor content"),
        };
        assert!(
            error.contains("ScreenCast"),
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
