//! The `cloud-vm` Azure module: the AWS model on the azurerm provider, from the plan. These
//! assert the Azure-specific facts; the characterization snapshots hold the whole output.

use isoloom_spec::{Spec, Target};
use isoloom_targets::Lab;

use isoloom_test_support::{hcl, hcl_attribute, hcl_blocks, hcl_body, hcl_literals, hcl_map, hcl_names, hcl_nested, hcl_one, hcl_text, hcl_values};

use crate::testing::{Outcome, generated, source as example};

/// The Azure module of `spec`.
fn azure(spec: &Spec) -> Outcome<String> {
    generated(&Lab::new(spec.clone()), Target::CloudVm, ".isoloom/cloud-vm/azure/main.tf")
}

/// The module's outputs, by name.
fn outputs(body: &hcl::Body) -> Vec<String> {
    body.blocks()
        .filter(|b| b.identifier() == "output")
        .filter_map(|b| b.labels().first().map(|l| l.as_str().to_owned()))
        .collect()
}

#[test]
fn azure_uses_azurerm_with_static_addresses() -> Outcome {
    let tf = azure(&example("hello-stack")?)?;
    let body = hcl_body(&tf);
    // The azurerm provider and an Azure VM, not an AWS instance.
    let terraform = hcl_blocks(&body, "terraform", &[]);
    let required = terraform.first().map(|t| hcl_nested(t.body(), "required_providers")).unwrap_or_default();
    let required_azurerm: Vec<(String, String)> = required.first().map(|r| hcl_map(r.body(), "azurerm")).unwrap_or_default();
    assert_eq!(
        required_azurerm,
        [
            ("source".to_owned(), "hashicorp/azurerm".to_owned()),
            ("version".to_owned(), "~> 5.8.0".to_owned())
        ]
    );
    assert!(hcl_names(&body, "resource", "azurerm_linux_virtual_machine").contains(&"web".to_owned()));
    assert!(hcl_names(&body, "resource", "aws_instance").is_empty(), "{tf}");
    // The module interface: a `region` var and ARM_SUBSCRIPTION_ID from the environment.
    assert_eq!(hcl_blocks(&body, "variable", &["region"]).len(), 1);
    let provider = hcl_blocks(&body, "provider", &["azurerm"]);
    assert_eq!(
        provider.first().map(|p| hcl_text(p.body(), "subscription_id")).as_deref(),
        Some("var.subscription_id")
    );
    let vm = hcl_one(&body, "azurerm_linux_virtual_machine", "web");
    assert_eq!(hcl_text(vm, "location"), "var.region");
    assert!(hcl_blocks(&body, "variable", &["location"]).is_empty(), "{tf}");
    // Every address of the spec is kept: a static private IP at the spec's octet.
    let nic = hcl_one(&body, "azurerm_network_interface", "web_app");
    let config = hcl_nested(nic, "ip_configuration");
    let config = config.first().ok_or("no ip_configuration")?.body();
    assert_eq!(
        (hcl_text(config, "private_ip_address_allocation"), hcl_text(config, "private_ip_address")),
        ("Static".into(), "10.60.0.10".into())
    );
    // A static public IP it is reached on (the `ip` output).
    assert_eq!(hcl_text(hcl_one(&body, "azurerm_public_ip", "web"), "allocation_method"), "Static");
    Ok(())
}

#[test]
fn azure_has_the_shared_outputs() -> Outcome {
    let tf = azure(&example("hello-stack")?)?;
    let body = hcl_body(&tf);
    // The module outputs, identical in name and shape to AWS.
    let names = outputs(&body);
    for wanted in ["machines", "ssh_users", "ip", "ready_file"] {
        assert!(names.iter().any(|n| n == wanted), "{wanted}: {names:?}");
    }
    let value = |name: &str| {
        hcl_blocks(&body, "output", &[name])
            .first()
            .and_then(|o| hcl_attribute(o.body(), "value"))
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    let ready = hcl_blocks(&body, "output", &["ready_file"]);
    assert_eq!(ready.first().map(|o| hcl_text(o.body(), "value")).as_deref(), Some("/var/lib/isoloom/ready"));
    // Each machine by its public address.
    let machines = hcl_blocks(&body, "output", &["machines"])
        .first()
        .map(|o| hcl_map(o.body(), "value"))
        .unwrap_or_default();
    assert!(
        machines.contains(&("web".to_owned(), "azurerm_public_ip.web.ip_address".to_owned())),
        "{} {machines:?}",
        value("machines")
    );
    Ok(())
}

#[test]
fn azure_gives_a_machine_on_several_networks_an_interface_on_each() -> Outcome {
    let tf = azure(&example("dmz-jump")?)?;
    let body = hcl_body(&tf);
    // The multi-homed gateway gets a NIC per network, forwarding on (AWS source/dest check off).
    for nic in ["gateway_dmz", "gateway_internal"] {
        assert_eq!(
            hcl_text(hcl_one(&body, "azurerm_network_interface", nic), "ip_forwarding_enabled"),
            "true",
            "{nic}"
        );
    }
    // Its extra interface is found by MAC: Azure reports it dash-separated and uppercase, so it
    // is lowered and colon-joined, and interpolated into the set-up (not left a literal). Inside
    // the interpolation the quotes are HCL's own, not escaped (terraform rejects `\"` there).
    let scripts: Vec<String> = hcl_literals(&body).into_iter().filter(|l| l.contains('\n')).collect();
    let by_mac = "${lower(replace(azurerm_network_interface.gateway_internal.mac_address, \"-\", \":\"))}";
    assert!(scripts.iter().any(|s| s.contains(by_mac)), "{tf}");
    assert!(!scripts.iter().any(|s| s.contains("$${lower(")), "{tf}");
    Ok(())
}

#[test]
fn azure_runs_windows_over_winrm_and_a_controller_runs_the_checks() -> Outcome {
    let tf = azure(&example("windows-hello")?)?;
    let body = hcl_body(&tf);
    // A Windows VM with the generated password, WinRM over HTTP.
    let vm = hcl_one(&body, "azurerm_windows_virtual_machine", "web01");
    assert_eq!(hcl_names(&body, "resource", "random_password"), ["windows"]);
    assert_eq!(hcl_values(vm, "protocol"), ["Http"], "its WinRM listener");
    let connections: Vec<String> = hcl_blocks(&body, "resource", &[])
        .iter()
        .flat_map(|r| hcl_nested(r.body(), "connection"))
        .map(|c| hcl_text(c.body(), "type"))
        .collect();
    assert!(connections.iter().any(|t| t == "winrm"), "{connections:?}");
    // A Windows-only environment: a Debian controller runs the checks from its public IP.
    assert_eq!(hcl_names(&body, "resource", "azurerm_linux_virtual_machine"), ["isoloom_controller"]);
    // Its checks run on the controller, reached on its public address.
    let checks = hcl_blocks(&body, "output", &["checks"]);
    let checks = checks.first().and_then(|o| hcl_attribute(o.body(), "value")).ok_or("no checks output")?;
    let hcl::Expression::Array(runners) = checks else {
        return Err(format!("checks: {checks}").into());
    };
    let on_controller: Vec<String> = runners
        .iter()
        .filter_map(|r| match r {
            hcl::Expression::Object(o) => {
                let field = |k: &str| o.iter().find(|(key, _)| key.to_string() == k).map(|(_, v)| v.to_string());
                (field("machine").as_deref() == Some("\"isoloom-controller\"")).then(|| field("host").unwrap_or_default())
            }
            _ => None,
        })
        .collect();
    isoloom_test_support::assert_every!(&on_controller, |h: &&String| *h == "azurerm_public_ip.isoloom_controller.ip_address");
    Ok(())
}
