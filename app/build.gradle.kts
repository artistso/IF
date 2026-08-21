plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.inkframe.studio"
    // Production builds remain on Android 16. Newer AndroidX releases that
    // require API 36.1/37 are pinned below until those SDKs are shipping-safe.
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "com.inkframe.studio"
        minSdk = 26
        targetSdk = 36
        versionCode = 60000
        versionName = "0.6.0-rust-bootstrap"
    }

    buildTypes {
        debug {
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = false
            isShrinkResources = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }
}

val buildRustDebug by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir)
    commandLine("bash", "scripts/build-rust-android.sh", "debug")
}

val buildRustRelease by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir)
    commandLine("bash", "scripts/build-rust-android.sh", "release")
}

tasks.matching { it.name == "mergeDebugJniLibFolders" }.configureEach {
    dependsOn(buildRustDebug)
}
tasks.matching { it.name == "mergeReleaseJniLibFolders" }.configureEach {
    dependsOn(buildRustRelease)
}

dependencies {
    // Core 1.18+ raises its compileSdk floor above API 36; 1.17.0 is the
    // newest production-safe Core line for this Android 16 shipping baseline.
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.activity:activity-compose:1.13.0")

    val composeBom = platform("androidx.compose:compose-bom:2026.06.01")
    implementation(composeBom)
    androidTestImplementation(composeBom)

    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
