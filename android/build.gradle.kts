import com.android.build.api.dsl.ApplicationExtension
import com.android.build.api.dsl.LibraryExtension

plugins {
    // AGP 9 ships Kotlin support built in; applying
    // `org.jetbrains.kotlin.android` alongside it is a hard error.
    id("com.android.application") version "9.3.2" apply false
    id("com.android.library") version "9.3.2" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.10" apply false
}

subprojects {
    plugins.withId("com.android.application") {
        extensions.configure<ApplicationExtension> {
            lint {
                abortOnError = true
                warningsAsErrors = true
                checkReleaseBuilds = true
                baseline = project.file("lint-baseline.xml")
                disable += setOf("AndroidGradlePluginVersion", "GradleDependency")
            }
        }
    }

    plugins.withId("com.android.library") {
        extensions.configure<LibraryExtension> {
            lint {
                abortOnError = true
                warningsAsErrors = true
                checkReleaseBuilds = true
            }
        }
    }
}
