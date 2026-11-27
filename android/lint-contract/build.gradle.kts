plugins {
    id("com.android.library")
}

android {
    namespace = "org.reprise.lintcontract"
    compileSdk {
        version = release(37) { minorApiLevel = 1 }
    }

    defaultConfig {
        minSdk = 26
    }
}
