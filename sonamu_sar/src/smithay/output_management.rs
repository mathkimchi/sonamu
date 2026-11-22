//! wlr-output-management (#158): clients (eg wdisplays, kanshi) see the monitors, and turn them on or off and move
//! them for this run (like niri, not saved; a `displays` change drops them). Changing a mode, transform, scale or
//! adaptive sync fails (#164, #165).

use crate::runner::AppletRunner;
use smithay::{
    reexports::{
        wayland_protocols_wlr::output_management::v1::server::{
            zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
            zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
            zwlr_output_head_v1::{self, AdaptiveSyncState, ZwlrOutputHeadV1},
            zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
            zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
        },
        wayland_server::{
            Client, DataInit, DisplayHandle, New, Resource, WEnum, protocol::wl_output::Transform,
        },
    },
    wayland::{Dispatch2, GlobalDispatch2},
};
use sonamu_ui::winit_backend::layout::{MonitorInfo, Placement};
use std::sync::Mutex;

/// The monitors as clients were last told, and the clients' changes
#[derive(Debug)]
pub(crate) struct OutputManagementState {
    managers: Vec<Manager>,
    monitors: Vec<MonitorInfo>,
    /// Of `monitors`; a configuration made before a change is cancelled
    serial: u32,
    /// What clients set, by monitor name: before the `displays` setting
    pub(crate) overrides: Vec<(String, Placement)>,
}

#[derive(Debug)]
struct Manager {
    manager: ZwlrOutputManagerV1,
    heads: Vec<Head>,
}

/// A monitor as one client sees it, with its one mode (the one it shows)
#[derive(Debug)]
struct Head {
    connector: String,
    head: ZwlrOutputHeadV1,
    mode: ZwlrOutputModeV1,
}

/// The global's
struct OutputManagerGlobal;

/// A head's and its mode's: the monitor's connector
struct Connector(String);

/// A configuration's: the serial it was made for, and what it sets
struct Configuration {
    serial: u32,
    state: Mutex<ConfigurationState>,
}

#[derive(Default)]
struct ConfigurationState {
    /// By connector; `None`: off
    heads: Vec<(String, Option<HeadChanges>)>,
    /// Applied or tested already
    used: bool,
}

/// What a configuration sets for a head it turns on
#[derive(Clone, Copy, Default)]
struct HeadChanges {
    position: Option<[i32; 2]>,
    /// Something Sonamu can't do (eg another scale)
    unsupported: bool,
}

/// A configuration head's: its connector and changes, shared with its configuration
struct ConfigurationHead {
    configuration: ZwlrOutputConfigurationV1,
    connector: String,
}

impl OutputManagementState {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        display.create_global::<AppletRunner, ZwlrOutputManagerV1, _>(4, OutputManagerGlobal);
        Self {
            managers: Vec::new(),
            monitors: Vec::new(),
            serial: 0,
            overrides: Vec::new(),
        }
    }

    /// The monitors changed: tells every client
    pub(crate) fn set_monitors(&mut self, monitors: Vec<MonitorInfo>, display: &DisplayHandle) {
        self.monitors = monitors;
        self.serial = self.serial.wrapping_add(1);
        for manager in &mut self.managers {
            sync(manager, &self.monitors, self.serial, display);
        }
    }

    /// Every connected monitor, as last told
    pub(crate) fn monitors(&self) -> &[MonitorInfo] {
        &self.monitors
    }

    fn monitor(&self, connector: &str) -> Option<&MonitorInfo> {
        self.monitors
            .iter()
            .find(|monitor| monitor.monitor.connector == connector)
    }
}

/// Tells the client the monitors (new heads, gone ones, and every head's state), then `done`
fn sync(manager: &mut Manager, monitors: &[MonitorInfo], serial: u32, display: &DisplayHandle) {
    let Some(client) = manager.manager.client() else {
        return;
    };
    let version = manager.manager.version();
    manager.heads.retain(|head| {
        let connected = monitors
            .iter()
            .any(|monitor| monitor.monitor.connector == head.connector);
        if !connected {
            head.mode.finished();
            head.head.finished();
        }
        connected
    });
    for info in monitors {
        let connector = &info.monitor.connector;
        if !manager
            .heads
            .iter()
            .any(|head| &head.connector == connector)
            && let Some(head) = new_head(&manager.manager, info, &client, display)
        {
            manager.heads.push(head);
        }
        let Some(Head { head, mode, .. }) = manager
            .heads
            .iter()
            .find(|head| &head.connector == connector)
        else {
            continue;
        };
        head.enabled(i32::from(info.position.is_some()));
        if let Some([x, y]) = info.position {
            head.current_mode(mode);
            head.position(x.cast_signed(), y.cast_signed());
            head.transform(Transform::Normal);
            head.scale(1.0);
            if version >= 4 {
                head.adaptive_sync(AdaptiveSyncState::Disabled);
            }
        }
    }
    manager.manager.done(serial);
}

/// Tells the client of the monitor: a new head and its mode, and what doesn't change
fn new_head(
    manager: &ZwlrOutputManagerV1,
    info: &MonitorInfo,
    client: &Client,
    display: &DisplayHandle,
) -> Option<Head> {
    let version = manager.version();
    let connector = &info.monitor.connector;
    let head = client
        .create_resource::<ZwlrOutputHeadV1, _, AppletRunner>(
            display,
            version,
            Connector(connector.clone()),
        )
        .ok()?;
    manager.head(&head);
    head.name(connector.clone());
    head.description(info.monitor.description());
    let [width, height] = info.physical_size.map(u32::cast_signed);
    head.physical_size(width, height);
    let mode = client
        .create_resource::<ZwlrOutputModeV1, _, AppletRunner>(
            display,
            version.min(3),
            Connector(connector.clone()),
        )
        .ok()?;
    head.mode(&mode);
    let [width, height] = info.monitor.size.map(u32::cast_signed);
    mode.size(width, height);
    mode.refresh(info.refresh_millihertz.cast_signed());
    mode.preferred();
    if version >= 2 {
        let [make, model, serial] = info.monitor.identity.clone();
        head.make(make);
        head.model(model);
        head.serial_number(serial);
    }
    Some(Head {
        connector: connector.clone(),
        head,
        mode,
    })
}

impl GlobalDispatch2<ZwlrOutputManagerV1, AppletRunner> for OutputManagerGlobal {
    fn bind(
        &self,
        state: &mut AppletRunner,
        display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        data_init: &mut DataInit<'_, AppletRunner>,
    ) {
        let manager = data_init.init(resource, ());
        let management = &mut state.smithay_state.output_management;
        let mut manager = Manager {
            manager,
            heads: Vec::new(),
        };
        sync(
            &mut manager,
            &management.monitors,
            management.serial,
            display,
        );
        management.managers.push(manager);
    }
}

impl Dispatch2<ZwlrOutputManagerV1, AppletRunner> for () {
    fn request(
        &self,
        state: &mut AppletRunner,
        _client: &Client,
        resource: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, AppletRunner>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                data_init.init(
                    id,
                    Configuration {
                        serial,
                        state: Mutex::default(),
                    },
                );
            }
            zwlr_output_manager_v1::Request::Stop => {
                resource.finished();
                state
                    .smithay_state
                    .output_management
                    .managers
                    .retain(|manager| &manager.manager != resource);
            }
            _ => {}
        }
    }

    fn destroyed(
        &self,
        state: &mut AppletRunner,
        _client: smithay::reexports::wayland_server::backend::ClientId,
        resource: &ZwlrOutputManagerV1,
    ) {
        state
            .smithay_state
            .output_management
            .managers
            .retain(|manager| &manager.manager != resource);
    }
}

// only `release`, which needs nothing
impl Dispatch2<ZwlrOutputHeadV1, AppletRunner> for Connector {
    fn request(
        &self,
        _state: &mut AppletRunner,
        _client: &Client,
        _resource: &ZwlrOutputHeadV1,
        _request: zwlr_output_head_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, AppletRunner>,
    ) {
    }
}
impl Dispatch2<ZwlrOutputModeV1, AppletRunner> for Connector {
    fn request(
        &self,
        _state: &mut AppletRunner,
        _client: &Client,
        _resource: &ZwlrOutputModeV1,
        _request: zwlr_output_mode_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, AppletRunner>,
    ) {
    }
}

impl Dispatch2<ZwlrOutputConfigurationV1, AppletRunner> for Configuration {
    fn request(
        &self,
        state: &mut AppletRunner,
        _client: &Client,
        resource: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, AppletRunner>,
    ) {
        use zwlr_output_configuration_v1::{Error, Request};
        let mut configuration = self.state.lock().unwrap();
        let mut configure = |head: &ZwlrOutputHeadV1, changes| {
            let Connector(connector) = head.data::<Connector>()?;
            if configuration
                .heads
                .iter()
                .any(|(other, _)| other == connector)
            {
                resource.post_error(Error::AlreadyConfiguredHead, "head configured twice");
                return None;
            }
            configuration.heads.push((connector.clone(), changes));
            Some(connector.clone())
        };
        match request {
            Request::EnableHead { id, head } => {
                if let Some(connector) = configure(&head, Some(HeadChanges::default())) {
                    data_init.init(
                        id,
                        ConfigurationHead {
                            configuration: resource.clone(),
                            connector,
                        },
                    );
                }
            }
            Request::DisableHead { head } => {
                configure(&head, None);
            }
            Request::Apply | Request::Test => {
                if std::mem::replace(&mut configuration.used, true) {
                    resource.post_error(Error::AlreadyUsed, "configuration used already");
                    return;
                }
                let management = &state.smithay_state.output_management;
                if self.serial != management.serial {
                    resource.cancelled();
                    return;
                }
                let unconfigured = management.monitors.iter().any(|monitor| {
                    !configuration
                        .heads
                        .iter()
                        .any(|(connector, _)| connector == &monitor.monitor.connector)
                });
                if unconfigured {
                    resource.post_error(Error::UnconfiguredHead, "every head must be configured");
                    return;
                }
                let Some(overrides) = overrides(management, &configuration.heads) else {
                    resource.failed();
                    return;
                };
                if matches!(request, Request::Apply) {
                    drop(configuration);
                    state.smithay_state.output_management.overrides = overrides;
                    state.sync_displays();
                }
                resource.succeeded();
            }
            _ => {}
        }
    }
}

/// The configuration's placements by monitor name, if Sonamu can show them: something on, nothing unsupported,
/// none overlapping
fn overrides(
    management: &OutputManagementState,
    heads: &[(String, Option<HeadChanges>)],
) -> Option<Vec<(String, Placement)>> {
    let mut shown: Vec<[i32; 4]> = Vec::new();
    let mut overrides = Vec::new();
    for (connector, changes) in heads {
        let monitor = management.monitor(connector)?;
        let placement = match changes {
            Some(HeadChanges {
                unsupported: true, ..
            }) => return None,
            Some(changes) => {
                let [x, y] = changes
                    .position
                    .or(monitor
                        .position
                        .map(|position| position.map(u32::cast_signed)))
                    .unwrap_or_default();
                let [width, height] = monitor.monitor.size.map(u32::cast_signed);
                let rect = [x, y, x + width, y + height];
                let overlaps = shown.iter().any(|&[left, top, right, bottom]| {
                    rect[0] < right && left < rect[2] && rect[1] < bottom && top < rect[3]
                });
                if overlaps {
                    return None;
                }
                shown.push(rect);
                Placement {
                    position: Some([x, y]),
                    primary: monitor.primary,
                    off: false,
                }
            }
            None => Placement {
                position: monitor
                    .position
                    .map(|position| position.map(u32::cast_signed)),
                primary: monitor.primary,
                off: true,
            },
        };
        overrides.push((monitor.name.clone(), placement));
    }
    (!shown.is_empty()).then_some(overrides)
}

impl Dispatch2<ZwlrOutputConfigurationHeadV1, AppletRunner> for ConfigurationHead {
    fn request(
        &self,
        state: &mut AppletRunner,
        _client: &Client,
        _resource: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, AppletRunner>,
    ) {
        use zwlr_output_configuration_head_v1::Request;
        let Some(configuration) = self.configuration.data::<Configuration>() else {
            return;
        };
        let monitor = state
            .smithay_state
            .output_management
            .monitor(&self.connector);
        let mut configuration = configuration.state.lock().unwrap();
        let Some((_, Some(changes))) = configuration
            .heads
            .iter_mut()
            .find(|(connector, _)| connector == &self.connector)
        else {
            return;
        };
        let supported = match request {
            Request::SetPosition { x, y } => {
                changes.position = Some([x, y]);
                true
            }
            Request::SetMode { mode } => {
                matches!(mode.data::<Connector>(), Some(Connector(connector)) if connector == &self.connector)
            }
            Request::SetCustomMode {
                width,
                height,
                refresh,
            } => monitor.is_some_and(|monitor| {
                monitor.monitor.size == [width, height].map(i32::cast_unsigned)
                    && (refresh == 0 || refresh.cast_unsigned() == monitor.refresh_millihertz)
            }),
            Request::SetTransform { transform } => transform == WEnum::Value(Transform::Normal),
            #[expect(clippy::float_cmp, reason = "only exactly 1 is supported")]
            Request::SetScale { scale } => scale == 1.0,
            Request::SetAdaptiveSync { state } => {
                state == WEnum::Value(AdaptiveSyncState::Disabled)
            }
            _ => true,
        };
        changes.unsupported |= !supported;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonamu_ui::winit_backend::layout::Monitor;

    fn management() -> OutputManagementState {
        let monitor = |connector: &str, position| MonitorInfo {
            monitor: Monitor {
                connector: connector.to_string(),
                identity: ["M", connector, "1"].map(str::to_string),
                size: [1000, 800],
            },
            name: format!("M {connector} 1"),
            physical_size: [0, 0],
            refresh_millihertz: 60_000,
            position,
            primary: position == Some([0, 0]),
        };
        OutputManagementState {
            managers: Vec::new(),
            monitors: vec![
                monitor("eDP-1", Some([0, 0])),
                monitor("DP-1", Some([1000, 0])),
            ],
            serial: 1,
            overrides: Vec::new(),
        }
    }

    fn at(position: [i32; 2]) -> HeadChanges {
        HeadChanges {
            position: Some(position),
            unsupported: false,
        }
    }

    #[test]
    fn moves_and_turns_off() {
        let heads = [
            ("eDP-1".to_string(), Some(at([0, 800]))),
            ("DP-1".to_string(), None),
        ];
        let overrides = overrides(&management(), &heads).unwrap();
        assert_eq!(overrides[0].1.position, Some([0, 800]));
        assert!(overrides[0].1.primary);
        assert!(overrides[1].1.off);
        assert_eq!(overrides[1].0, "M DP-1 1");
    }

    #[test]
    fn refuses_overlaps_all_off_and_unsupported() {
        let management = management();
        let overlapping = [
            ("eDP-1".to_string(), Some(at([0, 0]))),
            ("DP-1".to_string(), Some(at([500, 0]))),
        ];
        assert!(overrides(&management, &overlapping).is_none());
        let off = [("eDP-1".to_string(), None), ("DP-1".to_string(), None)];
        assert!(overrides(&management, &off).is_none());
        let scaled = [
            (
                "eDP-1".to_string(),
                Some(HeadChanges {
                    position: None,
                    unsupported: true,
                }),
            ),
            ("DP-1".to_string(), None),
        ];
        assert!(overrides(&management, &scaled).is_none());
    }
}
