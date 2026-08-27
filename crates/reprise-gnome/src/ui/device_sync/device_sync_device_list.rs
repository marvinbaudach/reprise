//! Reconciles detected MTP connections with durable remembered-device history.

use super::*;

impl DeviceSyncRuntime {
    pub(super) fn apply_devices(self: &Rc<Self>, descriptors: Vec<DeviceDescriptor>) {
        let previous_active_id = self
            .device_states
            .borrow()
            .iter()
            .find(|state| state.connected && state.session_state.opens_session())
            .map(|state| state.descriptor.id.clone());
        let remembered = self
            .device_states
            .borrow()
            .iter()
            .filter(|state| !state.connected && state.descriptor.persistent_id.is_some())
            .map(|state| reprise_core::device_sync::DetectedDevice {
                id: state.descriptor.id.clone(),
                name: state.settings.device_name.clone(),
            })
            .collect::<Vec<_>>();
        let known_names = self
            .device_states
            .borrow()
            .iter()
            .map(|state| {
                (
                    state.descriptor.id.clone(),
                    state.settings.device_name.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        let detected = descriptors
            .iter()
            .map(|descriptor| reprise_core::device_sync::DetectedDevice {
                id: descriptor.id.clone(),
                name: known_names
                    .get(&descriptor.id)
                    .cloned()
                    .unwrap_or_else(|| descriptor.name.clone()),
            })
            .collect::<Vec<_>>();
        let projection = reprise_core::device_sync::project_device_presence(
            previous_active_id.as_deref(),
            &detected,
            &remembered,
        );
        let order = projection
            .iter()
            .enumerate()
            .map(|(index, device)| (device.id.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut incoming = descriptors
            .into_iter()
            .map(|descriptor| (descriptor.id.clone(), descriptor))
            .collect::<HashMap<_, _>>();
        let mut inspect = Vec::new();
        let project;
        {
            let mut states = self.device_states.borrow_mut();
            for state in states.iter_mut() {
                if incoming.contains_key(&state.descriptor.id) || !state.connected {
                    continue;
                }
                state.connected = false;
                state.session_state = DeviceSessionState::Remembered;
                state.scanning = false;
                state.scan_generation = state.scan_generation.saturating_add(1);
                state.scan_error = None;
                state.ever_inspected = false;
                state.storage = DeviceStorageSnapshot::default();
                state.managed_files.clear();
                state.verified_managed_track_count = None;
                if state.machine.is_some() {
                    state.resume_initiator = state
                        .descriptor
                        .reconnectable
                        .then_some(state.active_initiator)
                        .flatten();
                    cancel_device_run(state);
                }
            }
            states.retain(|state| {
                incoming.contains_key(&state.descriptor.id)
                    || state.machine.is_some()
                    || state.resume_initiator.is_some()
                    || state.descriptor.persistent_id.is_some()
            });
            for projected in projection {
                let id = projected.id;
                let Some(descriptor) = incoming.remove(&id) else {
                    continue;
                };
                if let Some(state) = states.iter_mut().find(|state| state.descriptor.id == id) {
                    let was_connected = state.connected;
                    let owned_session = state.session_state.opens_session();
                    if let Err(error) = memory::adopt_detected_device_name(
                        &self.conn,
                        &mut state.settings,
                        &descriptor,
                    ) {
                        tracing::warn!(
                            device_id = descriptor.id,
                            %error,
                            "could not adopt the detected device name"
                        );
                    }
                    state.descriptor = descriptor;
                    state.connected = true;
                    state.session_state = projected.state;
                    if state.session_state.opens_session() && (!was_connected || !owned_session) {
                        inspect.push(id.clone());
                    }
                } else {
                    let (settings, target) = memory::load_device_memory(&self.conn, &descriptor)
                        .unwrap_or_else(|error| {
                            tracing::warn!(
                                device_id = descriptor.id,
                                %error,
                                "could not load device memory"
                            );
                            (
                                DeviceSettings::transient(&descriptor.id, &descriptor.name),
                                SyncTarget::default(),
                            )
                        });
                    let opens_session = projected.state.opens_session();
                    states.push(DeviceState::new(
                        descriptor,
                        settings,
                        target,
                        projected.state,
                    ));
                    if opens_session {
                        inspect.push(id);
                    }
                }
            }
            states.sort_by_key(|state| {
                order
                    .get(&state.descriptor.id)
                    .copied()
                    .unwrap_or(usize::MAX)
            });
            project = states
                .iter()
                .filter(|state| state.machine.is_none())
                .map(|state| state.descriptor.id.clone())
                .collect::<Vec<_>>();
        }
        self.notify();
        let projected_any = !project.is_empty();
        let mut recomputed_ids = HashMap::new();
        for id in project {
            let recomputed = match self.recompute_delta_silent(&id) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(
                        device_id = id,
                        %error,
                        "could not project Android sync playlists after a device presence change"
                    );
                    false
                }
            };
            recomputed_ids.insert(id, recomputed);
        }
        if projected_any {
            self.notify();
        }
        for id in inspect {
            // A device that is still mid-cancellation (machine not yet
            // cleared by finish_sync) is excluded from `project` above, so
            // it needs its own recompute attempt before inspection.
            let recomputed = match recomputed_ids.get(&id) {
                Some(&recomputed) => recomputed,
                None => match self.recompute_delta_silent(&id) {
                    Ok(()) => true,
                    Err(error) => {
                        tracing::warn!(
                            device_id = id,
                            %error,
                            "could not prepare Android sync playlists before device inspection"
                        );
                        false
                    }
                },
            };
            if let Some(device) = self
                .device_states
                .borrow_mut()
                .iter_mut()
                .find(|device| device.descriptor.id == id)
            {
                if recomputed {
                    device.library_dirty = false;
                }
                device.sync_phase = PlannedSyncPhase::ComputingDelta;
            }
            self.refresh_contents_on_connect(&id);
        }
    }
}
