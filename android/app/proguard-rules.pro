# JNA resolves classes and methods reflectively, so R8 must not touch them or
# the UniFFI bindings that sit on top of it.
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.**
