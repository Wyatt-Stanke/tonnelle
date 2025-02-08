#!/usr/bin/env bash

# Sync this folder to the server (ubuntu@129.80.117.126) in ~/freebind
rsync -avz --exclude 'target' --exclude 'node_modules' --exclude '.git' --exclude '.cargo' -e "ssh -i $(echo ~/*.key)" "$(pwd)/" ubuntu@129.80.117.126:~/freebind --delete
