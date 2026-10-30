plugins {
    kotlin("jvm") version "2.4.10"
    kotlin("plugin.allopen") version "2.4.10"
    id("org.jetbrains.kotlinx.benchmark") version "0.4.14"
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-benchmark-runtime:0.4.14")
    testImplementation(kotlin("test"))
}

// JMH requires @State classes to be open.
allOpen {
    annotation("org.openjdk.jmh.annotations.State")
}

tasks.test {
    useJUnitPlatform()
}

benchmark {
    targets {
        register("main")
    }
}
