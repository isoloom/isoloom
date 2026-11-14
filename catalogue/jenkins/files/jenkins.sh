#!/bin/bash
# SPDX-License-Identifier: Apache-2.0
# Written by Isoloom from the catalogue's jenkins entry: the jenkins/jenkins image's
# /usr/local/bin/jenkins.sh contract. At every start, each file of $REF (/usr/share/jenkins/ref,
# where a Dockerfile puts plugins, init scripts and configuration) is copied into $JENKINS_HOME
# unless it is there already; a file named `<name>.override` replaces `<name>` every time. Then
# Jenkins runs in the foreground: $JAVA_OPTS for Java, $JENKINS_OPTS and the arguments for Jenkins.
set -eo pipefail
: "${JENKINS_HOME:=/var/jenkins_home}"
: "${REF:=/usr/share/jenkins/ref}"
: "${JENKINS_WAR:=/usr/share/jenkins/jenkins.war}"
: "${COPY_REFERENCE_FILE_LOG:=$JENKINS_HOME/copy_reference_file.log}"

echo "--- Copying files at $(date)" >> "$COPY_REFERENCE_FILE_LOG"
if [ -d "$REF" ]; then
  while IFS= read -r -d '' f; do
    rel=${f#"$REF"/}
    target=$JENKINS_HOME/${rel%.override}
    if [ "$rel" != "${rel%.override}" ] || [ ! -e "$target" ]; then
      mkdir -p "$(dirname "$target")"
      cp -pR "$f" "$target"
      echo "copied $rel" >> "$COPY_REFERENCE_FILE_LOG"
    fi
  done < <(find "$REF" \( -type f -o -type l \) -print0)
fi
echo "--- Copied files finished at $(date)" >> "$COPY_REFERENCE_FILE_LOG"

# A first argument that isn't an option is a command of its own, as the image runs it.
if [ $# -gt 0 ] && [ "${1#-}" = "$1" ]; then
  exec "$@"
fi
# shellcheck disable=SC2086 # the options are lists, as the image reads them.
exec java -Duser.home="$JENKINS_HOME" $JAVA_OPTS -jar "$JENKINS_WAR" $JENKINS_OPTS "$@"
