# Vagrant configuration sources

The `*.txt` files list the settings and options Vagrant and its provider plugins accept,
extracted from these sources (names only; the sources themselves aren't copied here). Refresh
them with `python3 scripts/refresh-vagrant.py` after changing a version (or with
`ISOLOOM_REFRESH=latest` to try each repository's newest release), then run `cargo test`:
unclassified settings fail the coverage tests.

| List | Source | Version |
| --- | --- | --- |
| vm.txt | hashicorp/vagrant `plugins/kernel_v2/config/vm.rb` | v2.4.9 |
| virtualbox.txt | hashicorp/vagrant `plugins/providers/virtualbox/config.rb` | v2.4.9 |
| hyperv.txt | hashicorp/vagrant `plugins/providers/hyperv/config.rb` | v2.4.9 |
| docker.txt | hashicorp/vagrant `plugins/providers/docker/config.rb` | v2.4.9 |
| communicators.txt | hashicorp/vagrant `plugins/kernel_v2/config/ssh*.rb`, `plugins/communicators/{winrm,winssh}/config.rb` | v2.4.9 |
| provisioners.txt | hashicorp/vagrant `plugins/kernel_v2/config/vm_provisioner.rb` (`*`), `plugins/provisioners/*/config*` with the classes each inherits from | v2.4.9 |
| push.txt | hashicorp/vagrant `plugins/pushes/*/config.rb` | v2.4.9 |
| networks.txt | hashicorp/vagrant `website/content/docs/networking/*.mdx`, the VirtualBox and Hyper-V network actions, `compile_forwarded_ports.rb`; vagrant-libvirt `docs/configuration.markdown` | v2.4.9, 0.12.2 |
| folders.txt | hashicorp/vagrant `plugins/kernel_v2/config/disk.rb`, `website/content/docs/synced-folders/*.mdx` | v2.4.9 |
| vmware_desktop.txt | hashicorp/vagrant-vmware-desktop `lib/vagrant-vmware-desktop/config.rb` | desktop-v3.0.5 |
| parallels.txt | Parallels/vagrant-parallels `lib/vagrant-parallels/config.rb` | v2.4.7 |
| libvirt.txt | vagrant-libvirt/vagrant-libvirt `lib/vagrant-libvirt/config.rb` | 0.12.2 |
| utm.txt | naveenrajm7/vagrant_utm `lib/vagrant_utm/config.rb` | v0.1.6 |
| qemu.txt | ppggff/vagrant-qemu `lib/vagrant-qemu/config.rb` | v0.6.3 |
| vmware_esxi.txt | josenk/vagrant-vmware-esxi `lib/vagrant-vmware-esxi/config.rb` | v2.5.2 (latest tag) |
