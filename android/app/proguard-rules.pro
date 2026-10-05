# JNA resolves classes and methods reflectively, so R8 must not touch them or
# the UniFFI bindings that sit on top of it.
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.**

# Glance instantiates a widget button's callback from its class name when the
# button is tapped, so R8 must neither rename nor remove those classes.
-keep class * implements androidx.glance.appwidget.action.ActionCallback { <init>(); }

# Glance brings in WorkManager, whose Room database is created by reflection
# through its no-arg constructor. R8 keeps the class but strips that
# constructor, and the release build then crashes at start (#1128).
-keep class * extends androidx.room.RoomDatabase { <init>(); }
