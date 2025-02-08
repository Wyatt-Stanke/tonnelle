#!/usr/bin/env bash

sudo apt-get update
sudo apt-get upgrade -y
sudo apt-get install -y clang net-tools pkg-config libssl-dev python-is-python3
sudo apt-get install -y linux-tools-common linux-tools-generic "linux-tools-$(uname -r)"
# For building faster
sudo apt-get install -y sccache mold
# For building standard freebind
# sudo apt-get install -y gcc make libnetfilter-queue-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly -y
. "$HOME/.cargo/env"

# Faster rust compilation
if command -v sccache &>/dev/null && command -v mold &>/dev/null; then
    RUSTC_WRAPPER=$(which sccache)
    export RUSTC_WRAPPER
    mkdir -p .cargo
    cat <<EOT >.cargo/config.toml
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
EOT
else
    echo "sccache and mold not found"
fi

# For performance analysis
cargo install flamegraph

# Run scripts/startup.sh every time the system starts
# echo "@reboot bash $HOME/freebind/scripts/startup.sh" | crontab

# Setup gimli addr2line
git clone https://github.com/gimli-rs/addr2line /tmp/addr2line
cargo install --features bin --path /tmp/addr2line
cd ~ || exit
