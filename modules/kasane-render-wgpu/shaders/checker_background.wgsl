struct Checker {
    light: vec4<f32>,
    dark: vec4<f32>,
    tile_origin: vec4<f32>,
};

@group(0) @binding(0) var<uniform> checker: Checker;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(corners[index], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let column = i32(floor((position.x - checker.tile_origin.y) / checker.tile_origin.x));
    let row = i32(floor((position.y - checker.tile_origin.z) / checker.tile_origin.x));
    if (((column + row) & 1) == 0) {
        return checker.light;
    }
    return checker.dark;
}
