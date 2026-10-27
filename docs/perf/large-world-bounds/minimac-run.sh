#!/usr/bin/env bash
# Runs on MiniMac inside the booked window. usage: minimac-run.sh timing|profile
export PATH=/opt/homebrew/bin:$PATH
ulimit -n 10240
cd ~/perf/bounds || exit 1
mkdir -p results profiles
node --version
case "$1" in
  timing)
    WORLDS=world2066 ROUNDS="${ROUNDS:-5}" WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
      main=arms/base fix=arms/fix results/bounds-web.json > results/bounds-web.log 2>&1
    echo "EXIT=$?" >> results/bounds-web.log ;;
  profile)
    PROFILE_DIR=profiles WORLDS=world2066 ROUNDS=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
      named_main=arms/named-base named_fix=arms/named-fix results/profile-web.json > results/profile-web.log 2>&1
    echo "EXIT=$?" >> results/profile-web.log ;;
esac
