use super::*;
use crate::testing;

/// `user` (the access machine) and `web` on `front`, `cache` on the offline `back`, which
/// `front` reaches on 6379 only; `web` also has two addresses on `side`.
const LAB: &str = "version: 1
name: t
networks:
  front: { cidr: 10.1.0.0/24 }
  back: { cidr: 10.2.0.0/24, internet: false }
  side: { cidr: 10.3.0.0/24 }
reach:
  - { from: front, to: back, ports: [6379] }
machines:
  user: { access: true, networks: { front: 5 } }
  web:
    networks: { front: 10, side: [10, 11] }
    ports: { site: { port: 80, protocol: http }, shell: { port: 4444, dormant: true }, dns: { port: 53, protocol: udp } }
    app: { image: nginx }
  cache: { networks: { back: 20 }, ports: { redis: 6379, other: 7000 }, app: { image: redis } }
";

/// The names of the checks run from machine `machine`.
fn from<'a>(checks: &'a [Resolved], machine: &str) -> Vec<&'a str> {
    let at = Position::Machine(machine.to_owned());
    checks.iter().filter_map(|c| (c.position == at).then_some(c.name.as_str())).collect()
}

#[test]
fn a_service_answers_where_a_shared_network_or_reach_lets_through_and_nowhere_else() {
    let checks = derived(&testing::spec(LAB));
    assert!(checks.iter().all(|c| c.derived));
    assert_eq!(
        from(&checks, "user"),
        ["web:80 on front from user", "cache:6379 from user", "cache:7000 blocked from user"]
    );
    assert_eq!(from(&checks, "web"), ["cache:6379 from web", "cache:7000 blocked from web"]);
    assert_eq!(
        from(&checks, "cache"),
        [
            "web:80 on front blocked from cache",
            "web:80 on side blocked from cache",
            "web:80 at 10.3.0.11 blocked from cache",
            "web:4444 on front blocked from cache",
            "web:4444 on side blocked from cache",
            "web:4444 at 10.3.0.11 blocked from cache",
            "no internet from cache",
        ]
    );
}

#[test]
fn an_answer_is_a_page_or_a_connection_waited_for_and_a_block_tried_once() {
    let checks = derived(&testing::spec(LAB));
    let named = |n: &str| checks.iter().find(|c| c.name == n).cloned().expect("derived");
    let page = named("web:80 on front from user");
    assert_eq!(page.wait, 30);
    let Probe::Http { url, expect, .. } = page.probe else { panic!("a page") };
    assert_eq!(
        (url.host, url.port, url.https, expect),
        (
            Host::Machine {
                name: "web".into(),
                network: "front".into()
            },
            Some(80),
            false,
            HttpExpect::Any
        )
    );
    let open = named("cache:6379 from web");
    assert_eq!(
        (open.wait, open.probe),
        (
            30,
            Probe::Tcp {
                host: Host::Machine {
                    name: "cache".into(),
                    network: "back".into()
                },
                port: 6379,
                expect: TcpExpect::Open,
                exchange: TcpExchange::default()
            }
        )
    );
    let blocked = named("web:80 at 10.3.0.11 blocked from cache");
    assert_eq!(
        (blocked.wait, blocked.probe),
        (
            0,
            Probe::Tcp {
                host: Host::Literal("10.3.0.11".into()),
                port: 80,
                expect: TcpExpect::Blocked,
                exchange: TcpExchange::default()
            }
        )
    );
    let offline = named("no internet from cache");
    assert!(matches!(offline.probe, Probe::Http { expect: HttpExpect::Blocked, url: ref internet, .. } if internet.host == Host::Literal("1.1.1.1".into())));
}

#[test]
fn reach_opens_its_ports_from_its_networks_to_its_network_only() {
    let spec = testing::spec(LAB);
    let user = &spec.machines["user"];
    let cache = &spec.machines["cache"];
    assert!(allowed(&spec, user, "front", 80), "a shared network");
    assert!(allowed(&spec, user, "back", 6379) && !allowed(&spec, user, "back", 7000));
    assert!(!allowed(&spec, cache, "front", 80), "reach is one-way");
    assert!(!allowed(&spec, user, "side", 80), "nothing reaches side");
    let open = testing::spec(&LAB.replace("ports: [6379] }", "}"));
    assert!(allowed(&open, &open.machines["user"], "back", 7000), "a rule without ports opens every port");
}

#[test]
fn with_no_machine_to_run_them_every_service_answers_from_the_networks() {
    let spec = testing::spec(
        "version: 1\nname: t\nnetworks: { lan: { cidr: 10.9.0.0/24 } }\nmachines:\n  dc: { networks: { lan: [10, 12] }, os: windows-server@2022, ports: { rdp: 3389, ldap: { port: 389, dormant: true }, web: { port: 443, protocol: https }, dns: { port: 53, protocol: udp } } }\n",
    );
    let checks = derived(&spec);
    assert_eq!(checks, from_networks(&spec));
    let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "dc:3389 from the networks",
            "dc:3389 at 10.9.0.12 from the networks",
            "dc:443 from the networks",
            "dc:443 at 10.9.0.12 from the networks"
        ]
    );
    assert!(checks.iter().all(|c| c.position == Position::Networks && c.wait == 30));
    assert!(matches!(&checks[2].probe, Probe::Http { url, .. } if url.https && url.port == Some(443)));
}
