#!/usr/bin/env python3
"""Resolve a release, not an arbitrary shell argument; used by local and CI."""
import json
import re
import sys
import urllib.request


def select(requested):
    if not requested:
        request = urllib.request.Request(
            'https://api.github.com/repos/home-assistant/operating-system/releases/latest',
            headers={'Accept': 'application/vnd.github+json', 'User-Agent': 'k11c-ci'})
        with urllib.request.urlopen(request, timeout=30) as response:
            requested = json.load(response)['tag_name']
    if not re.fullmatch(r'[0-9]+\.[0-9]+', requested):
        raise ValueError('Expected a stable HAOS release such as 18.3')
    return requested


if __name__ == '__main__':
    print('HAOS_RELEASE=' + select(sys.argv[1] if len(sys.argv) > 1 else ''))
