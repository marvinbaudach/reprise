import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

val workspacePackage = rootProject.file("../Cargo.toml")
    .readText()
    .substringAfter("[workspace.package]")
    .substringBefore("\n[")

fun workspacePackageValue(name: String): String = Regex("(?m)^$name = \"([^\"]+)\"$")
    .find(workspacePackage)
    ?.groupValues
    ?.get(1)
    ?: error("Missing workspace.package $name")

val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties().apply {
    if (keystorePropertiesFile.isFile) {
        keystorePropertiesFile.inputStream().use { load(it) }
    }
}

fun releaseSigningValue(environmentName: String, propertyName: String): String? =
    System.getenv(environmentName)?.takeIf(String::isNotBlank)
        ?: keystoreProperties.getProperty(propertyName)?.takeIf(String::isNotBlank)

val releaseSigningValues = linkedMapOf(
    "REPRISE_KEYSTORE_PATH" to releaseSigningValue("REPRISE_KEYSTORE_PATH", "storeFile"),
    "REPRISE_KEYSTORE_PASSWORD" to releaseSigningValue("REPRISE_KEYSTORE_PASSWORD", "storePassword"),
    "REPRISE_KEYSTORE_ALIAS" to releaseSigningValue("REPRISE_KEYSTORE_ALIAS", "keyAlias"),
    "REPRISE_KEY_PASSWORD" to releaseSigningValue("REPRISE_KEY_PASSWORD", "keyPassword"),
)
val missingReleaseSigningValues = releaseSigningValues.filterValues { it == null }.keys
val hasAnyReleaseSigningValue = missingReleaseSigningValues.size < releaseSigningValues.size
val hasReleaseSigningConfig = missingReleaseSigningValues.isEmpty()
val requireReleaseSigning = System.getenv("REPRISE_REQUIRE_RELEASE_SIGNING") == "1"

if ((requireReleaseSigning || hasAnyReleaseSigningValue) && !hasReleaseSigningConfig) {
    error(
        "Release signing is incomplete; provide ${missingReleaseSigningValues.joinToString()} " +
            "through the REPRISE_KEYSTORE_* environment variables or android/keystore.properties",
    )
}

android {
    namespace = "de.reprise.spike"
    // AndroidX 1.19 refuses anything below 37, so the spike compiles against
    // the same API level the test device actually runs.
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.marvinbaudach.reprise"
        minSdk = 26
        targetSdk = 37
        versionCode = 75
        versionName = "0.1.75"
        buildConfigField("String", "REPRISE_CORE_VERSION", "\"${workspacePackageValue("version")}\"")
        buildConfigField("String", "REPRISE_CORE_LICENSE", "\"${workspacePackageValue("license")}\"")
        buildConfigField("String", "REPRISE_MOBILE_LICENSE", "\"GPL-3.0-or-later\"")
        ndk {
            // The spike runs on the connected arm64 device and on the
            // x86_64 emulator (`pixel10xl_api37`). Both are kept because the
            // emulator is the target that needs no hardware; the APK size
            // measured for P8 must still be read per ABI, not from this fat
            // build.
            abiFilters += "arm64-v8a"
            abiFilters += "x86_64"
        }
    }

    signingConfigs {
        if (hasReleaseSigningConfig) {
            create("releaseUpload") {
                storeFile = rootProject.file(requireNotNull(releaseSigningValues["REPRISE_KEYSTORE_PATH"]))
                storePassword = requireNotNull(releaseSigningValues["REPRISE_KEYSTORE_PASSWORD"])
                keyAlias = requireNotNull(releaseSigningValues["REPRISE_KEYSTORE_ALIAS"])
                keyPassword = requireNotNull(releaseSigningValues["REPRISE_KEY_PASSWORD"])
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            signingConfig = signingConfigs.getByName(
                if (hasReleaseSigningConfig) "releaseUpload" else "debug",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
        unitTests.all {
            // Robolectric 4.16.1 cannot instrument Java 26 class files and fails
            // with "Unsupported class file major version 70" on the ambient JVM.
            it.javaLauncher.set(javaToolchains.launcherFor {
                languageVersion.set(JavaLanguageVersion.of(21))
            })
            it.systemProperty("user.home", gradle.gradleUserHomeDir.absolutePath)
        }
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.08.00"))
    // Navigation is not part of the Compose BOM. 2.9.8 is the newest stable
    // Navigation release compatible with the BOM's stable Compose 1.11 line.
    implementation("androidx.navigation:navigation-compose:2.9.8")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material3:material3-window-size-class")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.core:core-ktx:1.19.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
    implementation("androidx.media3:media3-exoplayer:1.11.0")
    implementation("androidx.media3:media3-session:1.11.0")
    // UniFFI's Kotlin bindings call into the .so through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    testImplementation("junit:junit:4.13.2")
    // The @aar above ships JNA's dispatch stub as an Android jniLib, which the
    // packaged app needs and a JVM unit test cannot find: Robolectric runs on
    // the desktop JVM, where JNA looks for the stub as a classpath resource
    // under com/sun/jna/<os>-<arch>/. The plain jar carries that layout, so the
    // test classpath gets one it can actually load.
    testImplementation("net.java.dev.jna:jna:5.19.1")
    testImplementation(platform("androidx.compose:compose-bom:2026.08.00"))
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("org.robolectric:robolectric:4.16.1")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
