pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

// The live room (ADR 0038) is built with LiveKit's client, one of whose
// dependencies (its fork of the AudioSwitch library) is published only on
// JitPack. That repository is consulted for that one group, and only when
// the app being built opts into the service: every other build resolves
// from Google and Maven Central alone, as before.
val liveRooms = providers.gradleProperty("morningstar.services")
    .getOrElse("")
    .split(',')
    .any { it.trim() == "live" }

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        if (liveRooms) {
            maven("https://jitpack.io") {
                content { includeGroup("com.github.davidliu") }
            }
        }
    }
}

rootProject.name = "morningstar-host"
include(":app")
