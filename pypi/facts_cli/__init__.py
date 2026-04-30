"""facts-cli: A CLI for fact-driven development with coding agents."""

import os
import subprocess
import sys


def main():
    """Entry point that delegates to the prebuilt binary."""
    binary = os.path.join(os.path.dirname(__file__), "bin", _binary_name())
    if not os.path.isfile(binary):
        print(
            "Error: facts binary not found. "
            "The postinstall script may have failed.\n"
            "Try reinstalling: pip install --force-reinstall facts-cli",
            file=sys.stderr,
        )
        sys.exit(1)
    try:
        result = subprocess.run([binary] + sys.argv[1:])
        sys.exit(result.returncode)
    except KeyboardInterrupt:
        sys.exit(130)


def _binary_name():
    if sys.platform == "win32":
        return "facts.exe"
    return "facts"
