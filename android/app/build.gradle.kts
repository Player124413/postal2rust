plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// versionCode must grow with every build that is installed over an older one, or Android refuses
// the update. The number of commits on this branch does that (CI checks out the full history).
// The fallback is only used when git is missing (a source zip).
val gitCommitCount: Int = try {
    val p = ProcessBuilder("git", "rev-list", "--count", "HEAD")
        .directory(rootDir.parentFile)
        .redirectErrorStream(true)
        .start()
    val out = p.inputStream.bufferedReader().readText().trim()
    if (p.waitFor() == 0) out.toInt() else 1
} catch (e: Exception) {
    1
}

android {
    namespace = "com.postal2rust.game"
    compileSdk = 35

    defaultConfig {
        // Keep this id for the whole life of the app: a different id is a different app, and
        // the copied game and the saves would not be reachable from the update.
        applicationId = "com.postal2rust.game"
        minSdk = 26
        targetSdk = 35
        // The base keeps the number above every build already installed on phones (the branch
        // history was rewritten during debugging, so the raw commit count fell back to 26).
        versionCode = 1000 + gitCommitCount
        versionName = "0.3.0-debug.${1000 + gitCommitCount}"
        // 64-bit phones only: every Android phone sold for years has arm64-v8a.
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    // One fixed debug key, committed on purpose (app/keystore/debug.keystore, password "android").
    // Without it every machine signs with its own ~/.android/debug.keystore, and an APK built on
    // one machine cannot update an APK built on another ("App not installed: signatures do not match").
    signingConfigs {
        create("fixedDebug") {
            storeFile = file("keystore/debug.keystore")
            storeType = "pkcs12"
            storePassword = "android"
            keyAlias = "postal2rust-debug"
            keyPassword = "android"
        }
    }

    buildTypes {
        getByName("debug") {
            signingConfig = signingConfigs.getByName("fixedDebug")
            isDebuggable = true
        }
    }

    // The Rust library (libp2android.so) is produced by cargo-ndk into src/main/jniLibs.
    sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")

    buildFeatures {
        buildConfig = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.activity:activity:1.9.3")
    implementation("androidx.documentfile:documentfile:1.0.1")
    implementation("androidx.core:core:1.13.1")
}
