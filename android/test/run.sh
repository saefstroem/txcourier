#!/bin/sh
# usage: android/test/run.sh
#   Runs the checks on the development computer. The phone build does not run them.
#   JAVAC and JAVA name the JDK when it is not on PATH.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
src=$here/../src/dev/txcourier
out=$here/out

if grep -nE '^[[:space:]]+(public |private |protected |static |final |sealed |abstract )*(class|interface|record|enum) [A-Z]' "$src"/*/*.java; then
    echo "FAIL  a nested type: one type per file"
    exit 1
fi
rm -rf "$out"
"${JAVAC:-javac}" --release 17 -Xlint:all -Werror -d "$out" "$src"/core/*.java "$here"/*.java
"${JAVA:-java}" -cp "$out" CoreCheck
echo "ok    CoreCheck"
