//! The shell the self-hosted services start with, and the helper their clients wait with.
//!
//! A secret's value is never trusted to be plain: the configurations are written from the secret
//! files at each start, each value escaped for the format it lands in.
//! - `SeaweedFS`'s S3 configuration is JSON: `\`, `"` and a tab escaped.
//! - Kafka's JAAS line is a quoted string of Kafka's `JaasConfig` (Java's `StreamTokenizer`: `\`
//!   and `"` escaped), inside a `.properties` value (`java.util.Properties`: every `\` doubled
//!   again).
//! - Kafka's `--add-scram` splits on `,` and has no escape, so the password never goes in it:
//!   its salted form does (PBKDF2-HMAC-SHA-512 by `openssl kdf` from the password's hex, 4096
//!   iterations, base64).
//!
//! A line break in a value is refused at the start, with a message: no format here carries one.

/// The helper a client waits for a fact's file with: on a cloud, a service's files land once it
/// exists, after the client's own set-up.
pub const WAIT_HELPER: &str = "/etc/isoloom/services/wait-for";

/// How long a client waits for a fact's file by default, in seconds.
pub const WAIT_DEFAULT_S: u32 = 300;

/// The SCRAM iterations of the self-hosted broker's login (Kafka's least for SHA-512).
pub const SCRAM_ITERATIONS: u32 = 4096;

/// `wait-for FILE [SECONDS]`: until `FILE` exists and isn't empty, at most `SECONDS`
/// ([`WAIT_DEFAULT_S`] by default); exits 1, saying so, on the timeout.
#[must_use]
pub fn wait_helper() -> String {
    format!(
        "#!/bin/sh\n# wait-for FILE [SECONDS]: until FILE exists and isn't empty (a service's fact, which a cloud\n# delivers once it has made the service), at most SECONDS (default {WAIT_DEFAULT_S}). Exits 1 on the timeout.\nf=${{1:?usage: wait-for FILE [SECONDS]}}; t=${{2:-{WAIT_DEFAULT_S}}}; i=0\nwhile [ ! -s \"$f\" ]; do\n  [ \"$i\" -lt \"$t\" ] || {{ echo \"wait-for: $f didn't appear in $t s\" >&2; exit 1; }}\n  sleep 1; i=$((i + 1))\ndone\n"
    )
}

/// Refuses a value of `vars` holding a line break, naming `what`.
fn one_line(vars: &str, what: &str) -> String {
    format!("case \"{vars}\" in *'\n'*) echo '{what} holds a line break, which its configuration cannot carry' >&2; exit 1 ;; esac\n")
}

/// Writes `SeaweedFS`'s S3 configuration to `out`, its one admin identity the keys in `dir`
/// (`access-key`, `secret-key`), each escaped for JSON.
#[must_use]
pub fn seaweedfs_config(dir: &str, out: &str) -> String {
    format!(
        "json() {{ printf '%s' \"$1\" | sed -e 's/\\\\/\\\\\\\\/g' -e 's/\"/\\\\\"/g' -e 's/\t/\\\\t/g'; }}\na=$(cat {dir}/access-key); s=$(cat {dir}/secret-key)\n{check}mkdir -p \"$(dirname {out})\"\n(umask 077 && printf '{{\"identities\":[{{\"name\":\"app\",\"credentials\":[{{\"accessKey\":\"%s\",\"secretKey\":\"%s\"}}],\"actions\":[\"Admin\",\"Read\",\"Write\",\"List\",\"Tagging\"]}}]}}' \"$(json \"$a\")\" \"$(json \"$s\")\" > {out})\n",
        check = one_line("$a$s", "an S3 key"),
    )
}

/// Writes Kafka's broker and client configurations into `state` (`server.properties`,
/// `client.properties`), its SASL/SCRAM login the user and password in `dir`, advertised at
/// `host`; and the `--add-scram` argument of its first start (`scram.arg`), the password salted.
#[must_use]
pub fn kafka_config(dir: &str, host: &str, state: &str) -> String {
    let props = "server.properties";
    format!(
        "jaas_quote() {{ printf '%s' \"$1\" | sed -e 's/\\\\/\\\\\\\\/g' -e 's/\"/\\\\\"/g'; }}\nprops() {{ printf '%s' \"$1\" | sed -e 's/\\\\/\\\\\\\\/g'; }}\nu=$(cat {dir}/user); p=$(cat {dir}/password)\n{check}case \"$u\" in *,*|*]*|'\"'*'\"') echo 'the Kafka user holds `,` or `]`, or is quoted, which --add-scram cannot carry' >&2; exit 1 ;; esac\njaas=$(props \"org.apache.kafka.common.security.scram.ScramLoginModule required username=\\\"$(jaas_quote \"$u\")\\\" password=\\\"$(jaas_quote \"$p\")\\\";\")\nmkdir -p {state}/data\numask 077\ncat > {state}/{props} <<EOF\nprocess.roles=broker,controller\nnode.id=1\ncontroller.quorum.voters=1@127.0.0.1:9093\nlisteners=SASL_PLAINTEXT://0.0.0.0:9092,CONTROLLER://127.0.0.1:9093\nadvertised.listeners=SASL_PLAINTEXT://{host}:9092\nlistener.security.protocol.map=SASL_PLAINTEXT:SASL_PLAINTEXT,CONTROLLER:PLAINTEXT\ncontroller.listener.names=CONTROLLER\ninter.broker.listener.name=SASL_PLAINTEXT\nsasl.enabled.mechanisms=SCRAM-SHA-512\nsasl.mechanism.inter.broker.protocol=SCRAM-SHA-512\nlistener.name.sasl_plaintext.scram-sha-512.sasl.jaas.config=$jaas\nlog.dirs={state}/data\noffsets.topic.replication.factor=1\ntransaction.state.log.replication.factor=1\ntransaction.state.log.min.isr=1\nauto.create.topics.enable=false\nEOF\ncat > {state}/client.properties <<EOF\nsecurity.protocol=SASL_PLAINTEXT\nsasl.mechanism=SCRAM-SHA-512\nsasl.jaas.config=$jaas\nEOF\nhex() {{ od -An -tx1 | tr -d ' \\n'; }}\nsalt=$(head -c 16 /dev/urandom | base64 | tr -d '\\n')\nsalted=$(openssl kdf -keylen 64 -kdfopt digest:SHA512 -kdfopt hexpass:\"$(printf '%s' \"$p\" | hex)\" -kdfopt hexsalt:\"$(printf '%s' \"$salt\" | base64 -d | hex)\" -kdfopt iter:{SCRAM_ITERATIONS} -binary PBKDF2 | base64 | tr -d '\\n')\nprintf 'SCRAM-SHA-512=[name=%s,salt=%s,saltedpassword=%s,iterations={SCRAM_ITERATIONS}]' \"$u\" \"$salt\" \"$salted\" > {state}/scram.arg\n",
        check = one_line("$u$p", "the Kafka login"),
    )
}

#[cfg(test)]
mod tests;
