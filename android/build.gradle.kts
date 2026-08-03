plugins {
    // AGP 9 ships Kotlin support built in; applying
    // `org.jetbrains.kotlin.android` alongside it is a hard error.
    id("com.android.application") version "9.3.1" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.10" apply false
}
