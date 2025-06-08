#!/usr/bin/env bash

# Sync this folder to the server (ubuntu@129.80.117.126) in ~/tonnelle
rsync -avz --exclude 'target' --exclude 'node_modules' --exclude '.git' --exclude '.cargo' -e "ssh -i $(echo ~/.ssh/ssh-key-2025*)" "$(pwd)/" ubuntu@129.80.117.126:~/tonnelle --delete
