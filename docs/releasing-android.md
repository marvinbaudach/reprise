# Android release signing

Reprise uses one long-lived upload key for every published Android APK. Create
it once, store it outside the repository, and back it up before publishing the
first release.

## Create and back up the upload key

A PKCS12 keystore has exactly one password for both the keystore and its private
key entry. Although `keytool -genkeypair` accepts `-keypass`, it discards a value
that differs from `-storepass`, prints `Different store and key passwords not
supported for PKCS12 KeyStores`, and still exits successfully. Choose one strong
upload-key password and use that same value for both arguments. In Fish, read it
once without putting the value into shell history:

```fish
read --silent --prompt-str 'Upload-key password: ' REPRISE_UPLOAD_PASSWORD
echo

keytool -genkeypair \
  -keystore /secure/path/reprise-upload.jks \
  -storetype PKCS12 \
  -storepass "$REPRISE_UPLOAD_PASSWORD" \
  -alias reprise-upload \
  -keypass "$REPRISE_UPLOAD_PASSWORD" \
  -keyalg RSA \
  -keysize 4096 \
  -sigalg SHA256withRSA \
  -validity 10000 \
  -dname 'CN=Reprise, OU=Release, O=Reprise, L=Zurich, ST=Zurich, C=CH'
```

Adjust the distinguished name before creation if the publisher identity should
be different. Put the `.jks` file and its password in a password manager, and
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
  -storepass "$REPRISE_UPLOAD_PASSWORD" \
  -alias reprise-upload \
  | sed -n 's/^[[:space:]]*SHA256: //p'
```

Replace the complete placeholder in
`android/signing/upload-key-sha256.txt` with that single uppercase,
colon-separated SHA-256 line. The release job verifies the APK signature and
then compares its certificate with this tracked value; a valid signature from a
different key still fails.

## Configure GitHub Actions secrets

Set the four repository secrets from a trusted checkout. Gradle requires both
password inputs even though PKCS12 uses only one password, so
`ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_PASSWORD` must contain the same
upload-key password. The commands below stream the encoded key and both password
secrets without writing another file; only the alias command prompts for a
value:

```fish
base64 --wrap=0 /secure/path/reprise-upload.jks | gh secret set ANDROID_KEYSTORE_BASE64
printf '%s' "$REPRISE_UPLOAD_PASSWORD" | gh secret set ANDROID_KEYSTORE_PASSWORD
gh secret set ANDROID_KEY_ALIAS
printf '%s' "$REPRISE_UPLOAD_PASSWORD" | gh secret set ANDROID_KEY_PASSWORD
```

Enter `reprise-upload` for `ANDROID_KEY_ALIAS`. Both password secrets deliberately
hold the same value; do not replace either one with a distinct password. Never
print the password or base64 payload to a terminal, CI log, issue, or pull
request.

## Build a signed universal APK locally

Create the ignored `android/keystore.properties` with an absolute key path:

```properties
storeFile=/secure/path/reprise-upload.jks
storePassword=replace-with-the-upload-password
keyAlias=reprise-upload
keyPassword=replace-with-the-upload-password
```

Keep both `storePassword` and `keyPassword`: Gradle requires both properties,
but for this PKCS12 keystore they deliberately contain the same value.

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
