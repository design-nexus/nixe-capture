#!/usr/bin/env bash
# Remove capture (--purge also removes its settings and data).
here=$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec "$here/../../uninstall.sh" capture "$@"
