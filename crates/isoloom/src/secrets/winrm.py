# Runs powershell.exe on a Windows machine over WinRM with this process's standard input as
# its own: `isoloom run` delivers a machine's secrets with it (crates/isoloom/src/secrets/windows.rs).
# The endpoint and the account come in the environment, powershell's arguments as ours: no value
# is in either, they come on the standard input only.
import os
import sys

try:
    from winrm.protocol import Protocol
except ImportError:
    sys.stderr.write("pywinrm isn't installed for python3 on this host: python3 -m pip install --user pywinrm\n")
    sys.exit(3)

p = Protocol(
    endpoint=os.environ["ISOLOOM_WINRM_URL"],
    transport=os.environ["ISOLOOM_WINRM_AUTH"],
    username=os.environ["ISOLOOM_WINRM_USER"],
    password=os.environ["ISOLOOM_WINRM_PASSWORD"],
    server_cert_validation="ignore",
    read_timeout_sec=110,
    operation_timeout_sec=100,
)
data = sys.stdin.buffer.read()
shell = p.open_shell()
try:
    command = p.run_command(shell, "powershell.exe", sys.argv[1:], console_mode_stdin=False, skip_cmd_shell=True)
    p.send_command_input(shell, command, data, end=True)
    out, err, code = p.get_command_output(shell, command)
    p.cleanup_command(shell, command)
finally:
    p.close_shell(shell)
sys.stderr.write(err.decode("utf-8", "replace"))
sys.exit(code)
