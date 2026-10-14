//! `tools`: the helpers Isoloom adds on every network (a shell, a browser...), at the addresses
//! just below the controller.

use super::{Blocks, Problems, block, dns_label};
use crate::model::{Spec, TOOL_RECIPES};

/// Tools take the addresses just below the controller on every network: no machine there.
pub(super) fn reserved(spec: &Spec, blocks: &Blocks, p: &mut Problems) {
    if spec.tools.is_empty() {
        return;
    }
    for (mname, m) in &spec.machines {
        for (net, octet) in &m.networks {
            if let Some(c) = block(blocks, net)
                && (0..spec.tools.len()).any(|i| c.tool(i).octets()[3] == *octet)
            {
                p.add(
                    format!("machines.{mname}.networks.{net}"),
                    format!(
                        ".{octet} is reserved for the tools (the {} addresses below .{}) when `tools:` is set",
                        spec.tools.len(),
                        c.controller().octets()[3]
                    ),
                );
            }
        }
    }
}

/// Tools: names, a recipe or an image, ports; three at most.
pub(super) fn each(spec: &Spec, p: &mut Problems) {
    for (name, t) in &spec.tools {
        let at = format!("tools.{name}");
        if !dns_label(name) {
            p.add(&at, "tool names are kebab-case and start with a letter");
        }
        if spec.machines.contains_key(name) || spec.groups.contains_key(name) {
            p.add(&at, format!("`{name}` is already a machine's or a group's name"));
        }
        let recipe = TOOL_RECIPES.contains(&name.as_str());
        if !recipe && t.image.is_none() {
            p.add(
                format!("{at}.image"),
                format!("give a container image, or use a recipe: {}", TOOL_RECIPES.join(", ")),
            );
        }
        if recipe && t.image.is_some() {
            p.add(
                format!("{at}.image"),
                format!("`{name}` is a recipe with its own image; leave `image` out or choose another name"),
            );
        }
        if t.publish.is_some() && t.port.is_none() {
            p.add(format!("{at}.publish"), "say which `port` the tool serves");
        }
        if let Some(port) = t.publish
            && spec
                .machines
                .values()
                .any(|m| m.ports.iter().any(|s| s.publish == Some(port) && s.protocol.is_tcp()))
        {
            p.add(format!("{at}.publish"), format!("host port {port} is already published by a machine"));
        }
        if spec.tools.len() > 3 {
            p.add("tools", "at most 3 tools (each takes one of the addresses Isoloom reserves on every network)");
            break;
        }
    }
}
