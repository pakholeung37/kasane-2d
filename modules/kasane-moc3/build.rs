use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_purism_core)");
    let purism_root = Path::new("../purism-core");
    if !purism_root.join("include/PurismCore.h").exists() {
        println!(
            "cargo:warning=PurismCore headers not found at {:?}",
            purism_root
        );
        return;
    }

    println!("cargo:rerun-if-changed=../purism-core/include");
    println!("cargo:rerun-if-changed=../purism-core/src");

    let sources = [
        "src/core.c",
        "src/debug.c",
        "src/arena.c",
        "src/math2.c",
        "src/moc3.c",
        "src/verify.c",
        "src/model.c",
        "src/update.c",
        "src/param.c",
        "src/part.c",
        "src/deformer.c",
        "src/artmesh.c",
        "src/glue.c",
        "src/offscreen.c",
        "src/blendshape.c",
        "src/interpolate.c",
        "src/render.c",
    ];

    let mut build = cc::Build::new();
    build.include(purism_root.join("include"));
    build.include(purism_root.join("src"));

    for s in &sources {
        build.file(purism_root.join(s));
    }

    // Set warnings / flags
    build.flag_if_supported("-Wall");
    build.flag_if_supported("-Wextra");
    build.flag_if_supported("-Wno-unused-parameter");
    build.flag_if_supported("-Wno-sign-compare");

    build.compile("PurismCore");
    println!("cargo:rustc-cfg=has_purism_core");
}
