//! Tables about an environment, from its resolved snapshot (`isoloom report`): addressing,
//! ports, wiring and resources, as aligned text or Markdown. They read what
//! [`crate::resolved::resolve`] worked out, so they can't disagree with the generated files.

use serde_json::Value;

use crate::resolved::{entries, ports};
use crate::{Error, Result};

/// The reports `isoloom report` knows, each with what it shows.
pub const REPORTS: &[(&str, &str)] = &[
    ("addressing", "every machine's address on every network"),
    ("ports", "what answers on which port, and from which networks it may be reached"),
    (
        "wiring",
        "how each target lays the networks out: Docker blocks, VirtualBox and libvirt network names, Kubernetes labels",
    ),
    ("resources", "CPUs, memory and disk per machine, and the totals"),
];

/// A table: its header, then its rows.
type Table = (Vec<String>, Vec<Vec<String>>);

/// One report, as text (aligned columns) or Markdown.
///
/// # Errors
///
/// [`Error::UnknownReport`] when no report has this name (see [`REPORTS`]).
pub fn report(name: &str, r: &Value, markdown: bool) -> Result<String> {
    let (head, rows) = match name {
        "addressing" => addressing(r),
        "ports" | "services" => ports(r),
        "wiring" => wiring(r),
        "resources" => resources(r),
        other => {
            return Err(Error::UnknownReport {
                name: other.to_owned(),
                known: REPORTS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "),
            });
        }
    };
    Ok(if markdown { markdown_table(&head, &rows) } else { text_table(&head, &rows) })
}

/// Under Vagrant's Hyper-V provider the published ports (`127.0.0.1:<port>` in the ports
/// report) aren't there: Hyper-V forwards none. A note saying so and where they are instead,
/// when the environment publishes any.
#[must_use]
pub fn hyperv_ports_note(r: &Value) -> Option<String> {
    let published: Vec<String> = entries(r, "machines")
        .flat_map(|(m, v)| {
            entries(v, "ports").filter_map(move |(_, s)| {
                let port = s.get("port").and_then(Value::as_u64).unwrap_or(0);
                s.get("publish").and_then(Value::as_u64).map(|p| format!("127.0.0.1:{p} ({m}:{port})"))
            })
        })
        .collect();
    (!published.is_empty()).then(|| {
        format!(
            "Note: on Vagrant's Hyper-V provider no port is forwarded to this host, so {} {} not there: reach each service at its VM's address on the first switch (`vagrant ssh-config <machine>` shows it).\n",
            published.join(", "),
            if published.len() == 1 { "is" } else { "are" }
        )
    })
}

/// The text at the pointer `at` of `v` (`/addresses/lan`), or empty.
fn text(v: &Value, at: &str) -> String {
    v.pointer(at).and_then(Value::as_str).unwrap_or("").to_owned()
}

/// The header cells.
fn head(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| (*c).to_owned()).collect()
}

/// Every machine's address on every network, and the router's.
fn addressing(r: &Value) -> Table {
    let nets: Vec<&String> = entries(r, "networks").map(|(n, _)| n).collect();
    let mut header = vec!["machine".to_owned()];
    header.extend(nets.iter().map(|n| (*n).clone()));
    let row = |name: String, v: &Value| -> Vec<String> { std::iter::once(name).chain(nets.iter().map(|n| text(v, &format!("/addresses/{n}")))).collect() };
    let mut rows: Vec<Vec<String>> = entries(r, "machines").map(|(m, v)| row(m.clone(), v)).collect();
    if let Some(router) = r.get("router").filter(|v| v.is_object()) {
        rows.push(row("(router)".to_owned(), router));
    }
    (header, rows)
}

/// What answers on each port, and from which networks: a machine's own, and those a `reach`
/// rule lets in on that port.
fn ports(r: &Value) -> Table {
    let header = head(&["machine", "port", "name", "http", "published", "reachable from"]);
    let nets: Vec<&String> = entries(r, "networks").map(|(n, _)| n).collect();
    let rules: Vec<&Value> = r.get("reach").and_then(Value::as_array).into_iter().flatten().collect();
    let mut rows = Vec::new();
    for (m, v) in entries(r, "machines") {
        let own: Vec<&str> = entries(v, "addresses").map(|(n, _)| n.as_str()).collect();
        for (name, s) in entries(v, "ports") {
            let port = s.get("port").and_then(Value::as_u64).unwrap_or(0);
            let reaches = |from: &str| {
                own.contains(&from)
                    || rules.iter().any(|rule| {
                        rule.get("from").and_then(Value::as_str) == Some(from)
                            && own.contains(&rule.get("to").and_then(Value::as_str).unwrap_or(""))
                            && rule
                                .get("ports")
                                .and_then(Value::as_array)
                                .is_some_and(|p| p.is_empty() || p.iter().any(|x| x.as_u64() == Some(port)))
                    })
            };
            let from: Vec<&str> = nets.iter().map(|n| n.as_str()).filter(|n| reaches(n)).collect();
            let http = matches!(s.get("protocol").and_then(Value::as_str), Some("http" | "https"));
            rows.push(vec![
                m.clone(),
                ports::shown(s),
                name.clone(),
                if http { "yes".to_owned() } else { String::new() },
                s.get("publish").and_then(Value::as_u64).map(|p| format!("127.0.0.1:{p}")).unwrap_or_default(),
                from.join(", "),
            ]);
        }
    }
    (header, rows)
}

/// How each target lays the networks out: the Docker block, the VirtualBox and libvirt network
/// (Vagrant's name), the Kubernetes label (Kubernetes' own), the gateway and the router.
fn wiring(r: &Value) -> Table {
    let name = r.get("name").and_then(Value::as_str).unwrap_or("");
    let header = head(&[
        "network",
        "cidr",
        "docker block",
        "virtualbox / libvirt network",
        "kubernetes label",
        "gateway",
        "router",
        "internet",
    ]);
    let rows = entries(r, "networks")
        .map(|(n, v)| {
            let gateway = v
                .get("gateway")
                .and_then(Value::as_str)
                .map(|g| format!("{g} ({})", text(v, "/gateway_address")));
            vec![
                n.clone(),
                text(v, "/cidr"),
                text(v, "/docker_cidr"),
                isoloom_vagrant::network_name(name, n),
                isoloom_kubernetes::network_member(n),
                gateway.unwrap_or_default(),
                text(v, "/router"),
                if v.get("internet").and_then(Value::as_bool) == Some(false) {
                    "no".to_owned()
                } else {
                    "yes".to_owned()
                },
            ]
        })
        .collect();
    (header, rows)
}

/// Each machine's CPUs, memory, disk, OS and forms, then the totals.
fn resources(r: &Value) -> Table {
    let header = head(&["machine", "cpus", "memory (MB)", "disk (GB)", "os", "shapes"]);
    let mut rows = Vec::new();
    let mut totals = [0_u64; 3];
    for (name, v) in entries(r, "machines") {
        let size = ["cpus", "memory_mb", "disk_gb"].map(|k| v.pointer(&format!("/resources/{k}")).and_then(Value::as_u64).unwrap_or(0));
        for (total, n) in totals.iter_mut().zip(size) {
            *total += n;
        }
        let shapes: Vec<&str> = v
            .get("shapes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let [cpus, memory, disk] = size.map(|n| n.to_string());
        rows.push(vec![name.clone(), cpus, memory, disk, text(v, "/os"), shapes.join(", ")]);
    }
    let [cpus, memory, disk] = totals.map(|n| n.to_string());
    rows.push(vec!["total".into(), cpus, memory, disk, String::new(), String::new()]);
    (header, rows)
}

/// A Markdown table, `|` in a cell escaped.
fn markdown_table(head: &[String], rows: &[Vec<String>]) -> String {
    let mut lines = vec![format!("| {} |", head.join(" | ")), format!("|{}", " --- |".repeat(head.len()))];
    lines.extend(
        rows.iter()
            .map(|r| format!("| {} |", r.iter().map(|c| c.replace('|', "\\|")).collect::<Vec<_>>().join(" | "))),
    );
    format!("{}\n", lines.join("\n"))
}

/// Aligned text columns, each as wide as its widest cell.
fn text_table(head: &[String], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = head
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(String::len)
                .chain(std::iter::once(h.len()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: &[String]| -> String {
        let padded: Vec<String> = cells.iter().zip(&widths).map(|(c, w)| format!("{c:<w$}")).collect();
        format!("{}\n", padded.join("  ").trim_end())
    };
    std::iter::once(line(head)).chain(rows.iter().map(|r| line(r))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(cells: &[&str]) -> Vec<String> {
        head(cells)
    }

    #[test]
    fn a_text_table_aligns_its_columns_and_trims_each_line() {
        let text = text_table(&cells(&["machine", "port"]), &[cells(&["web", "80"]), cells(&["cache-server", ""])]);
        assert_eq!(text, "machine       port\nweb           80\ncache-server\n");
    }

    #[test]
    fn a_markdown_table_escapes_a_bar_in_a_cell() {
        let md = markdown_table(&cells(&["a", "b"]), &[cells(&["x|y", "z"])]);
        assert_eq!(md, "| a | b |\n| --- | --- |\n| x\\|y | z |\n");
    }

    #[test]
    fn an_unknown_report_names_the_known_ones() {
        let e = report("nope", &Value::Null, false).err().map(|e| e.to_string());
        assert_eq!(e.as_deref(), Some("no report named `nope`; one of: addressing, ports, wiring, resources"));
    }
}
