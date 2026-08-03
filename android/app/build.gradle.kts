plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "de.reprise.spike"
    // AndroidX 1.19 refuses anything below 37, so the spike compiles against
    // the same API level the test device actually runs.
    compileSdk = 37

    defaultConfig {
        applicationId = "de.reprise.spike"
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1"
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

    buildTypes {
        release {
            // Signed with the debug key on purpose: the spike only needs a
            // realistic *size*, not a distributable artifact.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.06.01"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.core:core-ktx:1.19.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.11.0")
    implementation("androidx.media3:media3-exoplayer:1.10.1")
    implementation("androidx.media3:media3-session:1.10.1")
    // UniFFI's Kotlin bindings call into the .so through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
