#!/bin/bash
set -euo pipefail

usage() {
    echo "Usage: scripts/measure-avatar-07a.sh <VelaApp PID>" >&2
}

if [[ $# -ne 1 || ! "$1" =~ ^[0-9]+$ ]]; then
    usage
    exit 2
fi

pid="$1"
if ! kill -0 "$pid" 2>/dev/null; then
    echo "error: VelaApp PID $pid is not running" >&2
    exit 1
fi

echo "Sampling VelaApp PID $pid for 60 seconds. Leave the Overview tab visible and idle."
sample=0
while [[ "$sample" -le 60 ]]; do
    metrics="$(ps -p "$pid" -o %cpu=,rss=)"
    if [[ -z "${metrics//[[:space:]]/}" ]]; then
        echo "error: VelaApp exited during sampling" >&2
        exit 1
    fi
    echo "$metrics"
    if [[ "$sample" -lt 60 ]]; then
        sleep 1
    fi
    sample=$((sample + 1))
done | awk '
    {
        cpu += $1
        rss += $2
        samples += 1
    }
    END {
        printf "samples=%d avg_cpu=%.2f%% avg_rss=%.2f MiB\n", samples, cpu / samples, rss / samples / 1024
    }
'
