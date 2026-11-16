#!/bin/sh
# usage: sh android/integrity.sh
#
#   Checks this checkout before you build it: a clean tree, what the app can reach, which node
#   requests and courier writes it can make, and its manifest. The expected values are written out
#   below, so any change to what the app can do also changes this file and shows up in the diff you
#   review. Read this script like build.sh. It does not replace reading the code.
set -u
export LC_ALL=C

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
failed=0

check() {
    if [ "$2" = "$3" ]; then
        echo "ok    $1"
    else
        echo "FAIL  $1"
        echo "      expected: $3"
        echo "      found:    $2"
        failed=1
    fi
}

lines_in_src() {
    (cd "$here/src" && grep -rcE "$1" dev | grep -v ':0$' | sort | tr '\n' ' ')
}

check "the checkout is clean" "$(git -C "$root" status --porcelain --untracked-files=normal)" ""
check "only the USB manager and HTTPS reach outside the app" \
    "$(lines_in_src 'Socket|URLConnection|toURL|java\.net|ProcessBuilder|getRuntime|exec\(|loadLibrary|ClassLoader|forName|java\.lang\.reflect|java\.lang\.invoke|getDeclared|setAccessible|getMethod|java\.nio\.file|FileOutputStream|FileWriter|RandomAccess|PrintWriter|new File\(|getSystemService|getContentResolver|SharedPreferences|startActivity|sendBroadcast|android\.util\.Log|WebView|TrustManager|HostnameVerifier')" \
    "dev/txcourier/app/HttpRpc.java:3 dev/txcourier/app/MainActivity.java:1 "
check "only these hosts are named in the app and build.sh" \
    "$(cd "$here" && grep -rhoE 'https?://[^"]+' src build.sh | sort -u | tr '\n' ' ')" \
    "https://1rpc.io/bnb https://1rpc.io/eth https://bsc-dataseed.bnbchain.org https://bsc-rpc.publicnode.com https://bscscan.com/tx/ https://dl.google.com/android/repository https://eth.drpc.org https://etherscan.io/tx/ https://rpc.flashbots.net "
check "only these node requests" \
    "$(grep -rhoE '"eth_[A-Za-z]+"' "$here/src" | sort -u | tr '\n' ' ')" \
    '"eth_feeHistory" "eth_maxPriorityFeePerGas" "eth_sendRawTransaction" '
check "courier writes and publishing are three calls, all in MainActivity" \
    "$(lines_in_src 'courier\.(put|clear)\(|Node\.publish\(')" \
    "dev/txcourier/app/MainActivity.java:3 "
check "the only permission is internet" \
    "$(grep -oE 'android\.permission\.[A-Z_]+' "$here/AndroidManifest.xml" | tr '\n' ' ')" \
    "android.permission.INTERNET "
check "the only component is the one screen" \
    "$(grep -oE '<(activity|service|receiver|provider)' "$here/AndroidManifest.xml" | tr '\n' ' ')" \
    "<activity "
check "no debug, cleartext, shared user or network config flags" \
    "$(grep -oE 'debuggable|usesCleartextTraffic|sharedUserId|networkSecurityConfig' "$here/AndroidManifest.xml")" \
    ""
check "USB access is limited to the courier's IDs" \
    "$(grep -oE 'vendor-id="[0-9]+" product-id="[0-9]+"' "$here/res/xml/device_filter.xml" | tr '\n' ' ')" \
    'vendor-id="4617" product-id="1" '

commit=$(git -C "$root" rev-parse HEAD)
if [ "$failed" != 0 ]; then
    echo "FAILED for commit $commit: find out why before building"
    exit 1
fi
echo "all checks passed for commit $commit, which must be the commit you reviewed"
