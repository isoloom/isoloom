//! A container's hardware: the host's devices (`devices`, long form) and its GPUs, reserved as a
//! device request (`deploy.resources.reservations.devices`), which every Compose version reads
//! (the newer `gpus` key says the same).

use isoloom_plan::hardware::Hardware;
use isoloom_spec::model::{GpuCount, Gpus};

use crate::file::{Deploy, DeviceRequest, Reservations, Service, ServiceDevice};

/// A device's permissions when the spec gives none: read, write, create the node.
const ALL_ACCESS: &str = "rwm";

/// The capability every GPU request carries: Docker hands the request to a GPU driver by it.
const GPU: &str = "gpu";

/// Adds the machine's devices and GPUs to its service.
pub(super) fn machine(hw: &Hardware, svc: &mut Service) {
    svc.devices = hw
        .devices
        .iter()
        .map(|d| ServiceDevice {
            source: d.host.clone(),
            target: d.guest().to_owned(),
            // Always written: Docker's runtime refuses a long-form device with none (it has no
            // default there, unlike the short form's `rwm`).
            permissions: d.permissions.clone().unwrap_or_else(|| ALL_ACCESS.to_owned()),
        })
        .collect();
    if let Some(g) = &hw.gpus {
        let deploy = svc.deploy.get_or_insert_with(Deploy::default);
        deploy.resources.reservations = Some(Reservations { devices: vec![request(g)] });
    }
}

/// The GPU request: `gpu` and the spec's capabilities, its driver, then a count or the ids.
fn request(g: &Gpus) -> DeviceRequest {
    let mut capabilities = vec![GPU.to_owned()];
    capabilities.extend(g.capabilities.iter().filter(|c| c.as_str() != GPU).cloned());
    let count = match g.count {
        Some(GpuCount::All) => Some(serde_yaml_ng::Value::from("all")),
        Some(GpuCount::Count(n)) => Some(serde_yaml_ng::Value::from(n)),
        None if g.ids.is_empty() => Some(serde_yaml_ng::Value::from(1)),
        None => None,
    };
    DeviceRequest {
        capabilities,
        driver: g.driver().to_owned(),
        count,
        device_ids: g.ids.clone(),
        options: g.options.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(yaml: &str) -> Hardware {
        let m: isoloom_spec::model::Machine = serde_yaml_ng::from_str(yaml).unwrap();
        Hardware::of(&m)
    }

    fn yaml(svc: &Service) -> String {
        serde_yaml_ng::to_string(svc).unwrap()
    }

    #[test]
    fn devices_are_written_in_long_form() {
        let mut svc = Service::default();
        machine(&hardware("devices: [/dev/fuse, '/dev/ttyUSB0:/dev/ttyS1:rw']"), &mut svc);
        assert_eq!(
            yaml(&svc),
            "devices:\n- source: /dev/fuse\n  target: /dev/fuse\n  permissions: rwm\n- source: /dev/ttyUSB0\n  target: /dev/ttyS1\n  permissions: rw\n"
        );
    }

    #[test]
    fn gpus_are_a_device_request_beside_the_limits() {
        let mut svc = Service::default();
        machine(
            &hardware("gpus: { count: all, capabilities: [compute, gpu], options: { virtualization: 'false' } }"),
            &mut svc,
        );
        assert_eq!(
            yaml(&svc),
            "deploy:\n  resources:\n    reservations:\n      devices:\n      - capabilities:\n        - gpu\n        - compute\n        driver: nvidia\n        count: all\n        options:\n          virtualization: 'false'\n"
        );
    }

    #[test]
    fn gpus_by_id_name_them_and_their_driver() {
        let mut svc = Service::default();
        machine(&hardware("gpus: { ids: ['0', GPU-1], driver: cdi }"), &mut svc);
        assert!(
            yaml(&svc).contains("driver: cdi\n        device_ids:\n        - '0'\n        - GPU-1\n"),
            "{}",
            yaml(&svc)
        );
    }

    #[test]
    fn gpus_without_a_count_or_ids_are_one() {
        let mut svc = Service::default();
        machine(&hardware("gpus: {}"), &mut svc);
        assert!(yaml(&svc).contains("count: 1\n"));
    }
}
