plugins {
    kotlin("jvm") version "2.2.0"
    kotlin("plugin.allopen") version "2.2.0"
    id("org.jetbrains.kotlinx.benchmark") version "0.4.14"
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-benchmark-runtime:0.4.14")
}

kotlin {
    jvmToolchain(21)
}

// JMH requires @State classes to be open.
allOpen {
    annotation("org.openjdk.jmh.annotations.State")
}

benchmark {
    targets {
        register("main")
    }
}
