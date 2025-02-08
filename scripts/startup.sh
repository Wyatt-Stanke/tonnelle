#!/usr/bin/env bash

ulimit -n 50000

sudo modprobe sit
sudo modprobe ipv6

sudo ifconfig sit0 up
sudo ifconfig sit0 inet6 tunnel ::209.51.161.14
sudo ifconfig sit1 up
sudo ifconfig sit1 inet6 add 2001:470:1f06:430::2/64
sudo route -A inet6 add ::/0 dev sit1
# Required for freebind
sudo ip -6 route add local 2001:470:8a72::/48 dev lo

# This might be incorrect
sudo sysctl net.ipv6.ip_nonlocal_bind=1

# freebind -r 2001:470:8a72::/48 -- curl -6 ifconfig.me
sudo iptables -I INPUT 5 -p tcp --dport 1080 -j ACCEPT
sudo iptables -I INPUT 5 -p tcp --dport 8080 -j ACCEPT
sudo iptables -t nat -A PREROUTING -p tcp --dport 80 -j REDIRECT --to-port 8080

# Fix kernel address maps restriction
sudo sysctl -w kernel.kptr_restrict=0
sudo sysctl -w kernel.perf_event_paranoid=0

# Util
alias esudo='sudo -E env "PATH=$PATH"'

# Setup faster rust compilation
if command -v sccache &>/dev/null && command -v mold &>/dev/null; then
    mkdir -p ~/.cargo
    cat <<EOT >~/.cargo/config.toml
rustc-wrapper = "$(which sccache)"

[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
EOT
fi
