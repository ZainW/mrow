#!/usr/bin/env python3
"""Puts two `Mrow --bench` results side by side: bench-compare.py results-before.json results-after.json"""
import json
import sys

before, after = (json.load(open(path)) for path in sys.argv[1:3])
after_by_name = {stat["name"]: stat for stat in after}


def median(stat):
    samples = sorted(stat["samples"])
    return samples[len(samples) // 2]


def p95(stat):
    samples = sorted(stat["samples"])
    return samples[min(len(samples) - 1, -(-len(samples) * 95 // 100) - 1)]


print("| | before (median / p95) | after (median / p95) | change |")
print("|---|---:|---:|---:|")
for stat in before:
    other = after_by_name.get(stat["name"])
    if other is None:
        continue
    unit = stat["unit"]
    b, a = median(stat), median(other)
    change = f"{(a - b) / b * 100:+.0f}%" if b else "n/a"
    if unit == "MB":
        print(f"| {stat['name']} | {b:.0f} MB | {a:.0f} MB | {change} |")
    else:
        print(f"| {stat['name']} | {b:.1f} / {p95(stat):.1f} ms | {a:.1f} / {p95(other):.1f} ms | {change} |")
