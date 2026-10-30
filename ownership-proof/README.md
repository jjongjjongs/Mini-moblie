# Package ownership proof

A tiny APK that proves to Android developer verification (Android Developer Console)
that the holder of the Mini Mobile signing key owns `com.jjongjjongs.minimobile`: the
same package name, signed with the same key as the app, with the console's identifier
in `assets/adi-registration.properties`. It is not an app to install.

`.github/workflows/ownership-proof.yml` builds it on a push to the `ownership-proof`
branch with the `ANDROID_KEYSTORE_*` secrets the app build uses, checks its certificate,
and attaches it to the `ownership-proof` release as `MiniMobile-ownership-proof.apk`.
The branch is not `main` or `claude/**`, so the app's own workflow (which replaces the
`mobile-dev` apk) does not run for it.
