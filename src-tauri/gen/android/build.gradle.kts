import groovy.json.JsonSlurper

fun findRustlsPlatformVerifierAndroidAar(): File {
    val cargoManifest = rootProject.file("../../Cargo.toml")
    val dependencyJson = providers.exec {
        workingDir = cargoManifest.parentFile
        commandLine(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-linux-android",
            "--manifest-path",
            cargoManifest.path,
        )
    }.standardOutput.asText.get()
    val metadata = JsonSlurper().parseText(dependencyJson) as Map<*, *>
    val verifierPackage = (metadata["packages"] as List<*>)
        .map { it as Map<*, *> }
        .first { it["name"] == "rustls-platform-verifier-android" }
    val manifestPath = File(verifierPackage["manifest_path"] as String)
    val version = verifierPackage["version"] as String
    val artifactDir = File(
        manifestPath.parentFile,
        "maven/rustls/rustls-platform-verifier/$version",
    )
    val aar = File(artifactDir, "rustls-platform-verifier-$version.aar")
    check(aar.isFile) { "rustls Android certificate verifier AAR not found: $aar" }
    return aar
}

extra["rustlsPlatformVerifierAndroidAar"] = findRustlsPlatformVerifierAndroidAar()

buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:1.9.25")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

tasks.register("clean").configure {
    delete("build")
}

