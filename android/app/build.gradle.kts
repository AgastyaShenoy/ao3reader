import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.ao3reader"
    compileSdk = 36

    defaultConfig {
        applicationId = "org.ao3reader"
        minSdk = 26
        targetSdk = 36
        versionCode = 4
        versionName = "2.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Signed with the debug key so it installs for testing; a store build needs a real key.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The Rust bridge (libao3core.so) is built by ../build-rust.ps1 into src/main/jniLibs.
    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

// No AndroidX or other libraries: only the platform's own UI classes, to keep memory low.
dependencies {
}
