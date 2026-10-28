//! Read installed capture components and advertised portal properties only.
//! No capture element instance, portal session, device or room is created here.

use std::time::Duration;

use gst::prelude::*;
use gstreamer as gst;

use crate::linux_share_capture::{MIN_GSTREAMER_VERSION, REQUIRED_CAPTURE_ELEMENTS};

#[derive(serde::Serialize)]
pub(crate) struct Component {
    name: &'static str,
    available: bool,
    reason: String,
    effect: &'static str,
}

#[derive(serde::Serialize)]
pub(crate) struct Capabilities {
    monitor: bool,
    camera: bool,
    input: bool,
}

#[derive(serde::Serialize)]
pub(crate) struct Prerequisites {
    components: Vec<Component>,
    capabilities: Capabilities,
    scope: &'static str,
}

fn capture_components() -> Vec<Component> {
    let initialized = gst::init().map_err(|e| e.to_string());
    let version = gst::version();
    let supported = initialized.is_ok() && (version.0, version.1) >= MIN_GSTREAMER_VERSION;
    let reason = match initialized.as_ref() {
        Err(error) => format!("GStreamer initialization unavailable: {error}"),
        Ok(()) if !supported => format!(
            "GStreamer {}.{}.{} is older than required {}.{}",
            version.0, version.1, version.2, MIN_GSTREAMER_VERSION.0, MIN_GSTREAMER_VERSION.1
        ),
        Ok(()) => format!(
            "GStreamer {}.{}.{} supports the bounded capture queue",
            version.0, version.1, version.2
        ),
    };
    let mut components = vec![Component {
        name: "gstreamer",
        available: supported,
        reason,
        effect: "Monitor sharing requires a supported native GStreamer runtime",
    }];
    for &name in REQUIRED_CAPTURE_ELEMENTS {
        let loaded = if initialized.is_ok() {
            gst::ElementFactory::find(name)
                .ok_or_else(|| format!("Required GStreamer element {name} is unavailable"))
                .and_then(|factory| {
                    factory.load().map(|_| ()).map_err(|e| {
                        format!("Required GStreamer element {name} could not load: {e}")
                    })
                })
        } else {
            Err(format!(
                "Cannot inspect {name}: GStreamer initialization failed"
            ))
        };
        components.push(Component {
            name,
            available: loaded.is_ok(),
            reason: loaded.err().unwrap_or_else(|| {
                format!("Native GStreamer element {name} is installed and loadable")
            }),
            effect: "Monitor sharing cannot start without this native capture component",
        });
    }
    components
}

async fn screencast_properties() -> Result<String, String> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|e| format!("ScreenCast session bus unavailable: {e}"))?;
    let result = async {
        let bus = zbus::Proxy::new(&connection, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus")
            .await.map_err(|e| format!("Cannot inspect the session bus: {e}"))?;
        // Bind property reads to the current unique owner. Unlike addressing
        // the well-known portal name directly, this cannot auto-start a stopped
        // portal service, even if its previous owner disappears during the read.
        let owner: String = bus.call("GetNameOwner", &("org.freedesktop.portal.Desktop",))
            .await.map_err(|_| "ScreenCast portal is not running on this session bus; no service was started".to_string())?;
        let portal = zbus::Proxy::new(&connection, owner.as_str(), "/org/freedesktop/portal/desktop", "org.freedesktop.portal.ScreenCast")
            .await.map_err(|e| format!("ScreenCast metadata unavailable: {e}"))?;
        let version: u32 = portal.get_property("version").await
            .map_err(|e| format!("ScreenCast version unavailable: {e}"))?;
        let sources: u32 = portal.get_property("AvailableSourceTypes").await
            .map_err(|e| format!("ScreenCast source-type metadata unavailable: {e}"))?;
        let cursors: u32 = portal.get_property("AvailableCursorModes").await
            .map_err(|e| format!("ScreenCast cursor metadata unavailable: {e}"))?;
        if version < 5 || sources & 1 == 0 || cursors & 2 == 0 {
            return Err(format!("ScreenCast v{version} does not advertise the required v5 monitor/embedded-cursor path"));
        }
        Ok(format!("ScreenCast v{version} advertises monitor capture with an embedded cursor; permission and frames were not requested"))
    }.await;
    let _ = connection.close().await;
    result
}

#[tauri::command]
pub(crate) async fn linux_native_prerequisites() -> Prerequisites {
    // Plugin registry work never runs on GTK's main thread and only loads
    // factory metadata; making an Element here could open a real device.
    let (native, portal) = tokio::join!(
        tokio::task::spawn_blocking(capture_components),
        tokio::time::timeout(Duration::from_secs(3), screencast_properties()),
    );
    let mut components = native.unwrap_or_else(|error| {
        vec![Component {
            name: "gstreamer",
            available: false,
            reason: format!("Native capture registry inspection failed: {error}"),
            effect: "Monitor capture prerequisites could not be determined",
        }]
    });
    let portal = portal.unwrap_or_else(|_| {
        Err("ScreenCast metadata query timed out; no permission was requested".to_string())
    });
    components.push(Component {
        name: "screencast-portal",
        available: portal.is_ok(),
        reason: portal.unwrap_or_else(|error| error),
        effect: "Monitor sharing requires an available native ScreenCast portal",
    });
    let monitor = components.iter().all(|component| component.available);
    Prerequisites {
        components,
        capabilities: Capabilities { monitor, camera: false, input: false },
        scope: "Installed components and advertised portal interfaces on this GNOME Wayland computer only. Availability is not capture permission, a first frame, Windows rendering, or cross-distro acceptance. Camera publication and desktop input are not implemented in this unit.",
    }
}
