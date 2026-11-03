#!/bin/bash
# Builds the Android APK (arm64, debug-signed = same key as installed app) then the Linux .deb.
export ANDROID_HOME=~/android-toolchain/sdk NDK_HOME=~/android-toolchain/sdk/ndk/27.0.12077973 JAVA_HOME=~/android-toolchain/jdk PATH=~/android-toolchain/jdk/bin:$PATH
cd ~/claude/passvault
echo "== android" && node_modules/.bin/tauri android build --debug --target aarch64 --apk && echo "ANDROID-OK" || echo "ANDROID-FAIL"
echo "== linux deb" && node_modules/.bin/tauri build --bundles deb && echo "DEB-OK" || echo "DEB-FAIL"
