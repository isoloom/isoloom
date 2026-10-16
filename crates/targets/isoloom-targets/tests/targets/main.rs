//! Generation across the backends, from the repository's examples and specs written in the
//! tests: what each target writes, what it refuses and why, the resolved snapshot, its reports
//! and the run manifests, the way the composition root puts them together.

mod appliances;
mod chart;
mod checks;
mod clones;
mod cloud_docker;
mod cloud_services;
mod cloud_vm_azure;
mod cloud_vm_digitalocean;
mod cloud_vm_gcp;
mod cloud_vm_linode;
mod cloud_vm_oci;
mod constraints;
mod defaults;
mod determinism;
mod disk;
mod docker_vm_manifest;
mod every_example;
mod external;
mod generate;
mod groups;
mod hardware;
mod instance;
mod machine_settings;
mod matrix;
mod message;
mod own_image_boot;
mod project_copy;
mod proxmox_controller;
mod reboot;
mod report;
mod resolved;
mod run_manifest;
mod shared_volumes;
mod tc;
mod testing;
mod tiers;
mod tools;
mod trunks;
mod vagrant_manifest;
mod vagrant_providers;
