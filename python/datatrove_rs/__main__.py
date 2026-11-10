"""`python -m datatrove_rs script.py [args...]`: run an unmodified datatrove pipeline script with `patch()` applied."""

import os
import runpy
import sys

from datatrove_rs import patch


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit("usage: python -m datatrove_rs script.py [args...]  |  python -m datatrove_rs -m module [args...]")
    patch()
    if sys.argv[1] == "-m" and len(sys.argv) > 2:
        sys.argv = sys.argv[2:]
        runpy.run_module(sys.argv[0], run_name="__main__", alter_sys=True)
    else:
        sys.argv = sys.argv[1:]
        sys.path.insert(0, os.path.dirname(os.path.abspath(sys.argv[0])))  # as `python script.py` does
        runpy.run_path(sys.argv[0], run_name="__main__")


main()
