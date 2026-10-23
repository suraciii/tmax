#!/usr/bin/env python3
"""Reject Python bytecode and cache directories in the Git index.

Tracked bytecode is a build artifact masquerading as source: it rots silently,
can shadow the file it was compiled from, and makes review diffs unreadable.
Anything matching must be removed from the index and ignored instead.
"""

from __future__ import annotations

import os
from pathlib import PurePosixPath
import subprocess
import sys


def tracked_paths() -> list[str]:
    """Every path in the Git index, in index order."""
    raw = subprocess.check_output(['git', 'ls-files', '-z'])
    return [os.fsdecode(name) for name in raw.split(b'\0') if name]


def is_python_cache(path: str) -> bool:
    """True for a bytecode file or anything inside a ``__pycache__`` tree."""
    parts = PurePosixPath(path).parts
    return '__pycache__' in parts or PurePosixPath(path).suffix in ('.pyc', '.pyo')


def violations(paths: list[str]) -> list[str]:
    return [path for path in paths if is_python_cache(path)]


def main() -> int:
    caches = violations(tracked_paths())
    if caches:
        print('Python cache files must not be tracked:', file=sys.stderr)
        for path in caches:
            print(f'  {path}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
