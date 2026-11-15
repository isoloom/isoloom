"""shell_find's and hcl's readers, on small made-up inputs. In the image (it has PyYAML):

    docker run --rm -v "$PWD/xtask/src/verify/runner:/r" -w /r <image> python3 -m unittest test_shell_find
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import hcl
import shell_find


class HclStrings(unittest.TestCase):
    def test_a_string_reads_its_escapes_and_each_interpolation_as_a_placeholder(self) -> None:
        text, end = hcl.read_string('"a\\"b\\n${var.x ? "y" : "z"}$${HOME}%{ if true }c%{ endif }\\u0024" tail', 0)
        self.assertEqual(text, 'a"b\n0${HOME}c$')
        self.assertEqual(end, len('"a\\"b\\n${var.x ? "y" : "z"}$${HOME}%{ if true }c%{ endif }\\u0024"'))

    def test_a_list_splits_on_its_own_commas_only(self) -> None:
        text = 'x = [\n  "a, b",\n  f("c", "d"),\n  "e",\n]\n'
        self.assertEqual(hcl.elements(text, text.index("=") + 1), ['"a, b"', 'f("c", "d")', '"e"'])


class Terraform(unittest.TestCase):
    MODULE = """resource "terraform_data" "web" {
  provisioner "remote-exec" {
    inline = [
      "set -e",
      var.n > 0 ? "a" : "b",
      "echo \\"${self.id}\\"",
    ]
  }
  provisioner "local-exec" {
    command = "mkdir -p \\"${path.module}/x\\""
  }
}

resource "terraform_data" "win" {
  connection {
    type = "winrm"
  }
  provisioner "remote-exec" {
    inline = ["powershell -Command \\"x\\""]
  }
}

resource "aws_instance" "web" {
  user_data = var.on ? "#!/bin/sh\\nshutdown -h +${var.m}\\n" : null
  doc       = format("#cloud-config\\n%s", yamlencode({ write_files = [{ content = "#!/bin/sh\\ntrue\\n" }] }))
}
"""

    def test_a_module_yields_its_commands_and_shell_strings_but_not_windows_or_documents(self) -> None:
        found = shell_find.module_file(self.MODULE)
        self.assertEqual(
            [(s.where, s.shell, s.text) for s in found],
            [
                ("terraform_data.web remote-exec", "sh", 'set -e\necho "0"\n'),
                ("terraform_data.web local-exec", "sh", 'mkdir -p "0/x"\n'),
                ("aws_instance.web.user_data", "sh", "#!/bin/sh\nshutdown -h +0\n"),
            ],
        )


class Vagrant(unittest.TestCase):
    VAGRANTFILE = """Vagrant.configure("2") do |config|
  config.vm.define "web" do |m|
    m.vm.provision "shell", name: "routes", inline: <<~'SH'
      cat > /etc/r.sh <<'R'
      #!/bin/sh
      ip route add x
      R
      sh /etc/r.sh
    SH
    m.vm.provision "shell", name: "hosts", inline: "echo \\"a\\" >> /etc/hosts"
  end
  config.vm.define "dc" do |m|
    m.vm.guest = :windows
    m.vm.provision "shell", name: "setup", inline: "Write-Output 'x'"
  end
end
"""

    def test_a_vagrantfile_yields_its_linux_scripts_and_the_scripts_they_write(self) -> None:
        found = shell_find.vagrantfile(self.VAGRANTFILE)
        self.assertEqual(
            [(s.where, s.shell, s.text) for s in found],
            [
                ('web: provision "routes"', "bash", "cat > /etc/r.sh <<'R'\n#!/bin/sh\nip route add x\nR\nsh /etc/r.sh\n"),
                ('web: provision "routes" > /etc/r.sh', "sh", "#!/bin/sh\nip route add x\n"),
                ('web: provision "hosts"', "bash", 'echo "a" >> /etc/hosts\n'),
            ],
        )


class VagrantReadsBeside(unittest.TestCase):
    def test_an_inline_script_reads_the_file_beside_the_vagrantfile_as_vagrant_does(self) -> None:
        text = 'config.vm.define "web" do |m|\n  m.vm.provision "shell", name: "checks script", inline: "cat > /x <<\'T\'\\n" + File.read(File.join(__dir__, "checks/web.sh")) + "\\nT\\n"\nend\n'
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "checks").mkdir()
            (Path(tmp) / "checks" / "web.sh").write_text("echo ok", encoding="utf-8")
            found = shell_find.vagrantfile(text, Path(tmp))
        self.assertEqual(found[0].text, "cat > /x <<'T'\necho ok\nT\n\n")


class CloudConfig(unittest.TestCase):
    def test_a_document_yields_its_commands_and_shell_files(self) -> None:
        doc = "#cloud-config\nruncmd:\n  - echo a\n  - [ls, '-l', 'a b']\nwrite_files:\n  - { path: /x.sh, content: 'true' }\n  - { path: /y.conf, content: 'k=v' }\n"
        found = shell_find.cloud_config(doc, "vm.user_data")
        self.assertEqual(
            [(s.where, s.shell, s.text) for s in found],
            [("vm.user_data runcmd", "sh", "echo a\nls -l 'a b'\n"), ("vm.user_data write_files /x.sh", "sh", "true")],
        )


if __name__ == "__main__":
    unittest.main()
