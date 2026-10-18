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

#[cfg(test)]
mod tests {
    use crate::testing::{lab, problems};

    /// The problems of machine `web` (its port 80 published on 8080), group `front` and `tools`.
    fn with(tools: &str) -> Vec<String> {
        problems(&lab(&format!(
            "machines:\n  web: {{ networks: {{ lan: 10 }}, docker: {{ image: nginx }}, ports: [{{ port: 80, name: site, publish: 8080 }}] }}\ngroups: {{ front: {{ members: [web] }} }}\ntools: {tools}\n"
        )))
    }

    #[test]
    fn sound_tools_pass() {
        let three = "{ shell: {}, ui: { image: x, port: 3000, publish: 3000 }, dns: { image: y, port: 53 } }";
        assert_eq!(with(three), Vec::<String>::new());
    }

    #[test]
    fn a_tools_address_is_left_free() {
        let found = problems(&lab(
            "machines:\n  web: { networks: { lan: 252 }, docker: { image: nginx } }\n  db: { networks: { lan: 251 }, docker: { image: x } }\ntools: { shell: {} }\n",
        ));
        assert_eq!(
            found,
            ["machines.web.networks.lan: .252 is reserved for the tools (the 1 addresses below .253) when `tools:` is set"]
        );
    }

    #[test]
    fn a_tool_is_named_apart_from_the_machines_and_groups() {
        assert_eq!(with("{ 1ui: { image: x } }"), ["tools.1ui: tool names are kebab-case and start with a letter"]);
        for taken in ["web", "front"] {
            let why = format!("tools.{taken}: `{taken}` is already a machine's or a group's name");
            assert_eq!(with(&format!("{{ {taken}: {{ image: x }} }}")), [why], "{taken}");
        }
    }

    #[test]
    fn a_tool_is_a_recipe_or_an_image() {
        assert_eq!(with("{ ui: {} }"), ["tools.ui.image: give a container image, or use a recipe: shell"]);
        assert_eq!(
            with("{ shell: { image: x } }"),
            ["tools.shell.image: `shell` is a recipe with its own image; leave `image` out or choose another name"]
        );
    }

    #[test]
    fn a_published_tool_serves_a_port_no_machine_publishes() {
        assert_eq!(
            with("{ ui: { image: x, publish: 3000 } }"),
            ["tools.ui.publish: say which `port` the tool serves"]
        );
        assert_eq!(
            with("{ ui: { image: x, port: 3000, publish: 8080 } }"),
            ["tools.ui.publish: host port 8080 is already published by a machine"]
        );
    }

    #[test]
    fn three_tools_at_most() {
        assert_eq!(
            with("{ shell: {}, a: { image: x }, b: { image: x }, c: { image: x } }"),
            ["tools: at most 3 tools (each takes one of the addresses Isoloom reserves on every network)"]
        );
    }
}
