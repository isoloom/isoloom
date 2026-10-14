//! The verdicts the Terraform tables share, one per family of resource types or arguments,
//! decided by the questions of docs/adr/0006-coverage-classification.md. A family is a reviewed
//! decision; a resource or argument no family names stays unclassified and fails the tests.

use crate::Support::{self, Emitted, Equivalent, NotPortable, Partial, Planned, Tooling};

/// A resource type, data source, argument or block Isoloom writes.
pub const WRITTEN: Support = Emitted {
    from: "(written by the generator)",
};
/// The resource types the cloud generators write.
pub const CLOUD_WRITTEN: Support = Emitted {
    from: "(written by the cloud-vm target, a VM per machine, or the cloud-docker target, one VM running the Compose file)",
};

// Resource types.

/// Making or sharing a machine image: a machine boots one made beforehand, by its id.
pub const IMAGES: Support = Equivalent {
    via: "machines.*.vm.image.<cloud>: a machine boots an image made beforehand, named by its id",
};
/// Another way to bind a private zone to the environment's network than the one written.
pub const ZONE_BINDING: Support = Equivalent {
    via: "dns.private: the zone is bound to the environment's network where it is written (the Route 53 zone's `vpc`, the Azure link, the Cloud DNS zone's networks, the view attached to the VCN's resolver)",
};
/// The cloud's own names for its VMs.
pub const CLOUD_NAMES: Support = Equivalent {
    via: "dns.private: the private zone names each machine (and its aliases) as the spec does, instead of the cloud's names made from IDs, addresses or labels",
};
/// The network's resolver.
pub const CLOUD_RESOLVER: Support = Equivalent {
    via: "dns.private: the network's default resolver answers the private zone (Azure's 168.63.129.16, Cloud DNS's default policy, the VCN's default DHCP options)",
};
/// Aliases as names of their own.
pub const ALIAS_RECORDS: Support = Equivalent {
    via: "dns.private: every alias is an A record of its own, at its machine's address",
};
/// Records the spec has nothing for.
pub const OTHER_RECORDS: Support = NotPortable {
    why: "mail, service and text records: Docker's DNS and the local targets' hosts files map names to addresses only",
};
/// Proxmox SDN's DNS.
pub const SDN_DNS: Support = Equivalent {
    via: "dns.private: each VM's hosts file carries the names under the zone's domain, its search domain (the SDN zone's DNS needs a PowerDNS server registered in the datacenter, outside the environment)",
};
/// A Proxmox LXC container's names.
pub const ROUTER_DNS: Support = Equivalent {
    via: "dns.private: the router's dnsmasq, a container's resolver, answers every name and alias under the zone's domain too",
};
/// Reverse names.
pub const REVERSE_DNS: Support = Planned {
    note: "reverse names: PTR records in a reverse zone per network, beside the private zone (`dns.private`)",
};
/// IPv6 names in the private zone.
pub const PRIVATE_DNS_IPV6: Support = Planned {
    note: "AAAA records in the private zone, with IPv6 networks on cloud-vm",
};
/// What a written private zone resource can also do, with no change for the environment.
pub const ZONE_SETTINGS: Support = Tooling {
    note: "a private zone's management (its SOA timers, protection, delegation, logging, a record's routing policy across endpoints): the names answer the same",
};
/// Public names.
pub const PUBLIC_DNS: Support = NotPortable {
    why: "public DNS and domains: local targets have no public names",
};
/// The account's ready-made network.
pub const DEFAULTS: Support = Tooling {
    note: "the account's default network: Isoloom builds its own",
};
/// Ways to place, scale or bill VMs.
pub const PLACEMENT: Support = Tooling {
    note: "how the cloud places, scales or bills the VMs (zones, templates, scaling groups, reservations, dedicated hosts, spot): no change in how the environment behaves",
};
/// Image pipelines.
pub const IMAGE_BUILD: Support = Tooling {
    note: "builds machine images: how an image is made, not what the environment runs",
};
/// Backups, snapshots and copies.
pub const BACKUP: Support = Tooling {
    note: "backs up, copies or restores disks: no change in how the environment behaves",
};
/// The account itself.
pub const ACCOUNT: Support = Tooling {
    note: "administers the cloud account (identities, policies, billing, quotas, auditing, monitoring): no change in how the environment behaves",
};
/// Software run in the VMs by the cloud.
pub const VM_AGENT: Support = NotPortable {
    why: "runs software in the VMs through this cloud's own agent (extensions, run commands, patching): no other VM target has one",
};
/// Managed services.
pub const MANAGED: Support = NotPortable {
    why: "one of this cloud's managed services (databases, queues, functions, clusters, storage...): no other VM target has it",
};
/// The cloud's network services.
pub const CLOUD_NETWORK: Support = NotPortable {
    why: "one of this cloud's network services (VPN and interconnect gateways, peering, private endpoints, load balancers, CDN): no other VM target has it",
};
/// A resource type kept under an older name.
pub const OLDER_NAME: Support = Tooling {
    note: "an older name of a resource type the provider still has: no change in behavior",
};

// Arguments and settings.

/// The provider block's settings.
pub const CREDENTIALS: Support = Tooling {
    note: "how Terraform reaches and authenticates to the API: no change in behavior",
};
/// The identifier the provider sets.
pub const ID: Support = Tooling {
    note: "the identifier the provider sets: no change in behavior",
};
/// Terraform's waits.
pub const TIMEOUTS: Support = Tooling {
    note: "how long Terraform waits: no change in behavior",
};
/// Names, labels and descriptions.
pub const TAGS: Support = Tooling {
    note: "names, labels and descriptions in the account: no change in behavior",
};
/// Where a resource lives in the account.
pub const LOCATION: Support = Tooling {
    note: "where in the account or region the resource lives: no change in behavior",
};
/// How Terraform changes the resource.
pub const PROTECT: Support = Tooling {
    note: "how Terraform creates, updates or deletes it (protection, waits, forced deletes): no change in behavior",
};
/// Another spelling of what Isoloom sets.
pub const ALIAS: Support = Tooling {
    note: "another way to write what Isoloom already sets, or a default it keeps: no change in behavior",
};
/// Source ports in firewall rules.
pub const SOURCE_PORTS: Support = Tooling {
    note: "a source port filter: reach rules name destination ports, from any source port",
};
/// The cloud's logs and metrics.
pub const MONITORING: Support = Tooling {
    note: "the cloud's own logging and monitoring: no change in behavior",
};
/// A cloud-only option.
pub const CLOUD: Support = NotPortable {
    why: "an option only this cloud has (its agents, metadata service, encryption keys, performance tiers): no other VM target has it",
};
/// The Proxmox server's own tuning.
pub const SERVER: Support = NotPortable {
    why: "tunes the Proxmox server's hardware for the VM (caches, NUMA, hugepages, SMBIOS, NIC model): no other target exposes it",
};
/// IPv6 on AWS: the VPC's block is Amazon's.
pub const AWS_IPV6: Support = NotPortable {
    why: "an AWS VPC's IPv6 block is Amazon's (or one registered with it), never the spec's: Isoloom refuses IPv6 networks on AWS",
};
/// IPv6 on Google Cloud: the VPC's range is Google's.
pub const GCP_IPV6: Support = NotPortable {
    why: "Google Cloud picks a VPC's internal IPv6 range itself (inside fd20::/20), never the spec's: Isoloom refuses IPv6 networks on Google Cloud",
};
/// IPv6 on Linode: the VPC's ranges are Linode's.
pub const LINODE_IPV6: Support = NotPortable {
    why: "Linode assigns a VPC's IPv6 ranges, never the spec's: Isoloom refuses IPv6 networks on Linode",
};
/// IPv6 on DigitalOcean: its VPCs are IPv4 only.
pub const DIGITALOCEAN_IPV6: Support = NotPortable {
    why: "DigitalOcean VPCs are IPv4 only (a droplet's IPv6 address is a public one): Isoloom refuses IPv6 networks on DigitalOcean",
};
/// IPv6 on Oracle Cloud: its VCNs take unique local prefixes, not written yet.
pub const OCI_IPV6: Support = Planned {
    note: "dual-stack VCNs with the spec's unique local block; Isoloom refuses IPv6 networks on Oracle Cloud until then",
};
/// A reserved or extra public IPv4 address.
pub const PUBLIC_IP: Support = NotPortable {
    why: "a reserved, shared or extra public address: local targets have no public addresses, and Isoloom reaches each machine at the one its VM gets",
};
/// A public IPv6 address.
pub const PUBLIC_IPV6: Support = NotPortable {
    why: "a public IPv6 address: local targets have none, and Isoloom reaches machines over IPv4",
};
/// IPv6 on Proxmox: the set-up steps give the address.
pub const PROXMOX_IPV6: Support = Equivalent {
    via: "networks.*.ipv6: a set-up step gives each VM its IPv6 address, the same on every VM target (not run on a real host)",
};
/// Data disks where the generator doesn't attach them yet (the `esxi` target refuses `disks`).
pub const DISKS: Support = Planned {
    note: "machines.*.disks on ESXi: an esxi_virtual_disk each, attached by slot (the target refuses data disks until then)",
};
/// Nested virtualization: the cloud's switch, or the block holding it.
pub const NESTED: Support = Emitted {
    from: "machines.*.nested_virtualization",
};
/// A GPU passed through to a VM.
pub const GPU_PASSTHROUGH: Support = Emitted {
    from: "machines.*.gpus.ids (a PCIe device per id: a PCI address, or a resource mapping)",
};
/// How the server hands a PCI device over.
pub const PASSTHROUGH_TUNING: Support = NotPortable {
    why: "how the Proxmox server hands its PCI device over (a vGPU slice, a ROM, the primary display): no other target exposes it",
};
/// A host's USB devices for a VM.
pub const USB: Support = NotPortable {
    why: "a host's USB device for a VM: `devices` reach containers only (a VM has its own virtual devices), and cloud VMs have none",
};
/// GPUs attached to a machine type that carries none.
pub const ACCELERATORS: Support = Equivalent {
    via: "machines.*.gpus.count, as a G2 machine type (its L4 GPUs attached)",
};
/// What a GPU instance needs: maintenance stops it instead of moving it.
pub const GPU_SCHEDULING: Support = Partial {
    from: "machines.*.gpus.count (`on_host_maintenance = TERMINATE`)",
    gap: "the other scheduling options: the cloud's own",
};
/// TPM and secure boot.
pub const TPM: Support = Planned {
    note: "with Windows 11's TPM and secure boot",
};
/// Several addresses on one network.
pub const ADDRESSES: Support = Planned {
    note: "with several addresses on one network",
};
/// CPU topology.
pub const CPUS: Support = Equivalent {
    via: "`resources.cpus`, through the machine type Isoloom picks",
};
/// NAT for private networks: every cloud machine has its own public address.
pub const NAT: Support = Equivalent {
    via: "each machine's own public address, through the cloud's internet gateway: every machine reaches the internet directly (Terraform sets it up over SSH at that address), so a NAT gateway would add a hop and a bill, not a way out",
};
/// Routes through a machine.
pub const ROUTER: Support = Planned {
    note: "with `networks.*.gateway`: a network routed through one of its machines",
};
/// A network's MTU.
pub const MTU: Support = Planned { note: "a network's MTU" };
/// VLAN tags on a cloud VM's interface.
pub const CLOUD_VLANS: Support = NotPortable {
    why: "a cloud carries no 802.1Q tags between VMs (Google Cloud's VLAN interfaces and Oracle Cloud's VLANs end at its own virtual switch): Isoloom gives each VLAN a subnet of its own, and a machine on several an interface on each",
};
/// Shared folders.
pub const SHARED: Support = Planned {
    note: "shared volumes (`volumes`) as host folders",
};
/// Start order on a server restart.
pub const ORDER: Support = Tooling {
    note: "the start order when the server restarts: `depends_on` orders the first start",
};
