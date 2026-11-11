plugins {
    id("com.android.application")
}

val appId = providers.gradleProperty("morningstar.appId").get()
val appName = providers.gradleProperty("morningstar.appName").get()
val libName = providers.gradleProperty("morningstar.libName").get()

android {
    // Namespace of the host's own Java classes; fixed, because the Rust JNI
    // symbols are named after it. The application id is what varies per app.
    namespace = "dev.morningstar.host"
    compileSdk = 36

    defaultConfig {
        applicationId = appId
        // API 24 is the first release where Vulkan 1.0 is part of the platform.
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "0.0.1"

        manifestPlaceholders["appName"] = appName
        manifestPlaceholders["libName"] = libName
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}
