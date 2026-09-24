#!/bin/zsh
set -euo pipefail

tools_dir=${0:A:h}
repo_dir=${tools_dir:h}
demo_dir=${repo_dir}/demos/gd-cubism-demo
gd_cubism_dir=${repo_dir}/modules/gd-cubism
purism_dir=${repo_dir}/modules/purism-core
purism_build=${repo_dir}/target/cubism-matrix/core/purism-v6
godot_bin=${GODOT_BIN:-/Applications/Godot_mono.app/Contents/MacOS/Godot}
core_archive=${purism_build}/libPurismCore.a
stage_tool=${repo_dir}/tools/stage_godot_addon.py

cmake -S ${purism_dir} -B ${purism_build} \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF \
  -DPURISM_CORE_ABI=v6
cmake --build ${purism_build} -j8

cd ${gd_cubism_dir}
CUBISM_CORE_PROVIDER=purism \
CUBISM_CORE_LIBRARY=${core_archive} \
  .venv/bin/python -m SCons platform=macos arch=arm64 target=template_debug -j8
python3 ${stage_tool} --core-provider purism ${demo_dir}

${godot_bin} --headless --editor --path ${demo_dir} --quit

cd ${demo_dir}
${godot_bin} --path ${demo_dir} res://main.tscn
