#!/bin/bash
docker buildx build --platform linux/arm64 -t registry.undertheprinter.com/poodle:latest --push .
kubectl rollout restart -n poodle deployment/poodle
