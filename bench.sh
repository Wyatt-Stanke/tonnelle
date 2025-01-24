#!/usr/bin/env bash
set -eo pipefail

DEBUG=true RUST_LOG=trace cargo bench
