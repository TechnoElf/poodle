#!/bin/sh
docker run --rm --user "$(id -u)":"$(id -g)" -v "$PWD":/tmp/build -v ~/.cargo/:"/home/$(whoami)/.cargo/" -w /tmp/build registry.undertheprinter.com/rust-arm-cross:latest cargo build --target aarch64-unknown-linux-gnu
docker build -t registry.undertheprinter.com/poodle:latest .
docker push registry.undertheprinter.com/poodle:latest
kubectl rollout restart -n poodle deployment/poodle
