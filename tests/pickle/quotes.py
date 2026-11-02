"""Writes the pickled quote of each method in a NIR dump as OUT/probe-<method>.tasty.

Usage: python3 -I quotes.py DUMP OUT
"""
import base64
import re
import sys


def main():
    method = None
    for line in open(sys.argv[1]):
        m = re.match(r"def @[^:]*::([A-Za-z]+)\(", line)
        if m:
            method = m.group(1)
        for q in re.finditer(r'"(XKGrH[A-Za-z0-9+/=]+)"', line):
            open(f"{sys.argv[2]}/probe-{method}.tasty", "wb").write(base64.b64decode(q.group(1)))


main()
