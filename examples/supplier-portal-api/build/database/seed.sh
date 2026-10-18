#!/bin/sh
# Stores the launch token in the database. It arrives as a file, never as an environment
# variable, and goes in as a hex literal, so no character of it can break the statement.
set -eu
token=$(od -An -v -tx1 /run/lab/launch-token | tr -d ' \n')
mysql -h 127.0.0.1 -P 3207 -u root <<SQL
CREATE DATABASE IF NOT EXISTS portal;
CREATE TABLE IF NOT EXISTS portal.launch (token VARBINARY(255) NOT NULL);
INSERT INTO portal.launch VALUES (0x$token);
SQL
