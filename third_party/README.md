# Third-party dependencies

This directory is the repository-wide home for local third-party source and
binary dependencies. Proprietary Cubism SDK contents are ignored by Git and
must not be committed.

Install the Native SDK at:

```text
third_party/CubismSdkForNative-5-r.5/
```

`modules/gd-cubism` and `benchmarks/cubism-matrix` both use this shared copy.
Set `CUBISM_SDK_ROOT` to use a different SDK location. A locally modified Cubism
Framework can be placed at `third_party/CubismNativeFramework/` or selected with
`CUBISM_FRAMEWORK_ROOT`.

The Live2D models used by the repository are inventoried in
[`models/README.md`](../models/README.md). Keep model files outside the SDK
under `models/local/`; SDK sample models remain in their vendor package.
