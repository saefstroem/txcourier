# txcourier
RP2350-based hardware wallet and Ledger wallet manager for secure multisig operations.


### Running TXCourier on an Android: 1. Prepare the phone

- Use the phone for nothing else. Keep Developer options and USB debugging off.
- Install Termux from its GitHub releases page, https://github.com/termux/termux-app/releases,
  opened in the phone's browser. Take the newest release's `arm64-v8a` APK, for example
  `termux-app_v0.118.3+github-debug_arm64-v8a.apk`. Don't use the Google Play listing, which is an
  unofficial build, or an APK from anywhere else.
- When Android asks whether the browser may install apps, allow it for this install. Then turn it
  off again: Settings, Apps, the ⋮ menu, Special access, Install unknown apps.
- GitHub builds of Termux are signed with a test key that Termux publishes, so anyone can make an
  APK that Android accepts as a Termux update. Only ever update Termux from the same releases page,
  and decline any other prompt to install or update Termux.
- In Termux:

```sh
pkg install git openjdk-21 aapt d8 apksigner zip unzip curl
```

### 2. Fetch the commit you reviewed

On github.com, in the phone's browser, open the commit you mean to build and note its SHA. Then:

```sh
git clone https://github.com/saefstroem/txcourier.git
cd txcourier
git checkout <sha>
```

Cloning runs no code from the repository, but running its scripts does, so inspect before you run
anything.

### 3. Inspect

Only `android/` reaches the phone. The firmware is Rust and is never built there. `android/test` holds checks that run on the development computer only,
and `build.sh` never touches it.

Read these in full:

| File | What it decides |
|---|---|
| `android/AndroidManifest.xml` | What the app may do: internet, USB host, one screen |
| `android/res/xml/device_filter.xml` | The courier's USB IDs, vendor 4617 and product 1 |
| `android/res/values/themes.xml` | Colours and font only |
| `android/build.sh` | Everything that runs in Termux |
| `android/integrity.sh` | The checks below |
| `android/src/dev/txcourier/app/` | The screens, the USB link and the network client |

`android/src/dev/txcourier/core/` is plain Java with nothing from Android: the transaction record the
app hands the courier, the courier's wire protocol, and the fee and publish logic. The record names
the asset, ETH, BNB, USDT, USDC or PAXG, and the chain. The courier looks that pair up in its own
built-in list, `crates/protocol/src/token/known.rs`, builds the actual transaction with its own Rust
code, and the Ledger shows what it signs. Compare the contract addresses in that list with each
issuer's site or the block explorer: the Ledger rejects a forged descriptor, but a genuine one for
the wrong contract would pass.

Then read `android/integrity.sh` and run it. Like `build.sh`, it runs in Termux, so read it first.

```sh
sh android/integrity.sh
```

It confirms the checkout is clean, and that the app names nothing beyond the USB manager, HTTPS, the
six nodes and two explorer links in `Chain.java`, and Google's download server in `build.sh`. It
checks the node requests, the courier writes, the manifest and the courier's USB IDs against values
written out in the script. Every line must read `ok`, and the last line names the commit, which must
be the one you reviewed.

Its expected values are spelled out, so any change to what the app can do also changes
`integrity.sh` and shows up in the diff. It does not replace reading the files above. For example, it can't see that each courier write in
`MainActivity.java` only runs from a confirmation, which acts on a long press.

### 4. Build and install

```sh
sh android/build.sh
```

- The first build downloads `platform-35_r02.zip`, about 64 MB, from Google. It stops unless the
  file's SHA-1 matches both Google's listing and the value pinned in `build.sh`.
- The first build also creates your signing key, `android/keystore/txcourier.jks`, and asks for a
  password, which every later build asks for again. Android only lets an APK signed with the same
  key replace the installed app. Losing the key only means uninstalling txcourier before installing
  a rebuilt one, since the app stores nothing.
- Before opening the installer, `build.sh` checks the APK itself: package `dev.txcourier`, internet
  as the only permission, `MainActivity` as the only screen, and no files beyond the code, the
  manifest, the device filter and the compiled resources. Anything else stops the build.
- The last line reads `built .../txcourier.apk from commit <sha>`. It must be the commit you
  reviewed, without `-dirty`.

Android asks whether Termux may install apps. Allow it to install txcourier, then turn it off again
in the same Settings screen as the browser's. Between builds, nothing in Termux can then start an
install unless you turn it back on. The app's menu shows the commit it was built from.

### 5. Update

Review only what changed since the commit you last built:

```sh
git fetch
git log --oneline <built>..<new>
git diff <built> <new> -- android
```

Check out the new commit, rerun `sh android/integrity.sh`, then build.
