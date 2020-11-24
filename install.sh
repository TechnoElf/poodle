#!/bin/sh

cargo build
sudo systemctl stop poodle || true
sudo cp ./target/debug/poodle /usr/bin/.
sudo cp ./poodle.service /etc/systemd/system/.
sudo systemctl daemon-reload
sudo systemctl enable poodle
sudo systemctl start poodle
