use super::*;

#[test]
fn the_vm_is_readied_then_compose_starts_in_it_all_to_its_address() {
    let ip = Template::from(Placeholder::Output(SnakeName::parse(IP).unwrap()));
    let steps = into_vm(&ssh_run(ip, USER), &RelPath::parse(SETUP).unwrap(), "../../..").unwrap();
    let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["docker", "archive", "project"]);
    let Run::Action { with, .. } = &steps[0].run else { panic!() };
    let host = with.iter().find(|(k, _)| k.as_str() == "host").map(|(_, v)| v.to_string());
    assert_eq!(host.as_deref(), Some("${outputs.ip}"));
}
