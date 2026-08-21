# Android release signing

Reprise uses one long-lived upload key for every published Android APK. Create
it once, store it outside the repository, and back it up before publishing the
first release.

## Create and back up the upload key

Choose strong, distinct store and key passwords. In Fish, read them without
putting either value into shell history:

```fish
read --silent --prompt-str 'Keystore password: ' REPRISE_KEYSTORE_PASSWORD
echo
read --silent --prompt-str 'Key password: ' REPRISE_KEY_PASSWORD
echo

keytool -genkeypair \
  -keystore /secure/path/reprise-upload.jks \
  -storetype PKCS12 \
  -storepass "$REPRISE_KEYSTORE_PASSWORD" \
  -alias reprise-upload \
  -keypass "$REPRISE_KEY_PASSWORD" \
  -keyalg RSA \
  -keysize 4096 \
  -sigalg SHA256withRSA \
  -validity 10000 \
  -dname 'CN=Reprise, OU=Release, O=Reprise, L=Zurich, ST=Zurich, C=CH'
```

Adjust the distinguished name before creation if the publisher identity should
be different. Put the `.jks` file and both passwords in a password manager, and
put a second encrypted copy on offline storage. Do not keep the only copy on the
development computer.

Losing this keystore is permanent: every installed copy trusts the original
certificate, so an APK signed with a replacement key cannot update it. Users
would have to uninstall Reprise, lose app-local state, and install a new
application identity.

## Pin the expected certificate

Print the certificate fingerprint with an English label:

```fish
env LC_ALL=C keytool -list -v \
  -keystore /secure/path/reprise-upload.jks \
  -storetype PKCS12 \
  -storepass "$REPRISE_KEYSTORE_PASSWORD" \
  -alias reprise-upload \
  | sed -n 's/^[[:space:]]*SHA256: //p'
```

Replace the complete placeholder in
`android/signing/upload-key-sha256.txt` with that single uppercase,
colon-separated SHA-256 line. The release job verifies the APK signature and
then compares its certificate with this tracked value; a valid signature from a
different key still fails.

## Configure GitHub Actions secrets

Set the four repository secrets from a trusted checkout. The first command
streams the encoded key without writing another file; the other three commands
prompt for their values:

```fish
base64 --wrap=0 /secure/path/reprise-upload.jks | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set ANDROID_KEYSTORE_PASSWORD
gh secret set ANDROID_KEY_ALIAS
gh secret set ANDROID_KEY_PASSWORD
```

Enter `reprise-upload` for `ANDROID_KEY_ALIAS`. Never print a password or the
base64 payload to a terminal, CI log, issue, or pull request.

## Build a signed universal APK locally

Create the ignored `android/keystore.properties` with an absolute key path:

```properties
storeFile=/secure/path/reprise-upload.jks
storePassword=replace-with-the-store-password
keyAlias=reprise-upload
keyPassword=replace-with-the-key-password
```

Restrict the file to the current user. It and all `*.jks`/`*.keystore` files are
gitignored, but that is not a substitute for keeping the key outside the
checkout.

Generate both native libraries and the UniFFI bindings, then require release
signing for the Gradle build. These are Fish commands; set the SDK/NDK paths for
the current machine:

```fish
set -gx ANDROID_HOME /path/to/android-sdk
set -gx ANDROID_NDK_HOME /path/to/android-ndk

env ANDROID_TARGET=aarch64-linux-android ANDROID_ABI=arm64-v8a ANDROID_API=26 \
  scripts/android-build.sh
env ANDROID_TARGET=x86_64-linux-android ANDROID_ABI=x86_64 ANDROID_API=26 \
  scripts/android-build.sh
env REPRISE_REQUIRE_RELEASE_SIGNING=1 \
  android/gradlew -p android --no-daemon :app:assembleRelease
```

The universal APK is
`android/app/build/outputs/apk/release/app-release.apk`. Before sharing it,
verify both the signing certificate and package metadata with the `apksigner`
and `aapt2` from the newest installed Android build-tools directory:

```fish
set build_tools (find "$ANDROID_HOME/build-tools" -mindepth 1 -maxdepth 1 \
  -type d -printf '%f\n' | sort -V | tail -1)
"$ANDROID_HOME/build-tools/$build_tools/apksigner" verify --print-certs \
  android/app/build/outputs/apk/release/app-release.apk
"$ANDROID_HOME/build-tools/$build_tools/aapt2" dump badging \
  android/app/build/outputs/apk/release/app-release.apk
```

An unconfigured local `assembleRelease` deliberately falls back to debug
signing for size measurements. Publication always sets
`REPRISE_REQUIRE_RELEASE_SIGNING=1`, so that fallback cannot reach a release.
