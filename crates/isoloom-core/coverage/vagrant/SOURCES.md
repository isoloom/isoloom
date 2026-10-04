# Vagrant configuration sources

The `*.txt` files list the settings each Vagrant config class accepts, extracted from these
sources (names only; the sources themselves aren't copied here). Refresh them with
`python3 scripts/vagrant-settings.py` after changing a version, then run `cargo test`:
unclassified settings fail the coverage tests.

| List | Source | Version |
| --- | --- | --- |
| vm.txt | hashicorp/vagrant `plugins/kernel_v2/config/vm.rb` | v2.4.9 |
| virtualbox.txt | hashicorp/vagrant `plugins/providers/virtualbox/config.rb` | v2.4.9 |
| hyperv.txt | hashicorp/vagrant `plugins/providers/hyperv/config.rb` | v2.4.9 |
| vmware_desktop.txt | hashicorp/vagrant-vmware-desktop `lib/vagrant-vmware-desktop/config.rb` | desktop-v3.0.5 |
| parallels.txt | Parallels/vagrant-parallels `lib/vagrant-parallels/config.rb` | v2.4.7 |
| libvirt.txt | vagrant-libvirt/vagrant-libvirt `lib/vagrant-libvirt/config.rb` | 0.12.2 |
| utm.txt | naveenrajm7/vagrant_utm `lib/vagrant_utm/config.rb` | v0.1.6 |
| qemu.txt | ppggff/vagrant-qemu `lib/vagrant-qemu/config.rb` | v0.6.3 |
