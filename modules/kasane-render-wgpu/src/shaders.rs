pub(super) const BASIC_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let multiplied = source.rgb * draw.multiply_color.rgb;
    let rgb = multiplied + draw.screen_color.rgb - multiplied * draw.screen_color.rgb;
    let alpha = source.a * draw.opacity;
    if (draw.blend_modes.z == 1u) {
        return vec4<f32>(rgb * alpha, 0.0);
    }
    if (draw.blend_modes.z == 2u) {
        return vec4<f32>(rgb * alpha + vec3<f32>(1.0 - alpha), 1.0);
    }
    return vec4<f32>(rgb * alpha, alpha);
}
"#;

pub(super) const COMPOSITE_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let rgb = source.rgb * draw.multiply_color.rgb
        + draw.screen_color.rgb * source.a
        - source.rgb * draw.screen_color.rgb;
    return vec4<f32>(rgb * draw.opacity, source.a * draw.opacity);
}
"#;

pub(super) const MASKED_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var mask_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let multiplied = source.rgb * draw.multiply_color.rgb;
    let rgb = multiplied + draw.screen_color.rgb - multiplied * draw.screen_color.rgb;
    let alpha = source.a * draw.opacity * mask_alpha(input.mask_point);
    if (draw.blend_modes.z == 1u) {
        return vec4<f32>(rgb * alpha, 0.0);
    }
    if (draw.blend_modes.z == 2u) {
        return vec4<f32>(rgb * alpha + vec3<f32>(1.0 - alpha), 1.0);
    }
    return vec4<f32>(rgb * alpha, alpha);
}
"#;

pub(super) const MASKED_COMPOSITE_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var mask_texture: texture_2d<f32>;
@group(2) @binding(1) var mask_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(main_texture, main_sampler, input.uv);
    let rgb = source.rgb * draw.multiply_color.rgb
        + draw.screen_color.rgb * source.a
        - source.rgb * draw.screen_color.rgb;
    let mask = mask_alpha(input.mask_point);
    return vec4<f32>(rgb * draw.opacity * mask, source.a * draw.opacity * mask);
}
"#;

pub(super) const MASK_SHADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, textureSample(main_texture, main_sampler, input.uv).a);
}
"#;

pub(super) const EXTENDED_HEADER: &str = r#"
struct DrawUniform {
    target_size: vec2<f32>,
    padding: vec2<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: f32,
    padding_end: vec3<f32>,
    mask_bounds: vec4<f32>,
    mask_flags: vec4<u32>,
    blend_modes: vec4<u32>,
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    view_origin: vec4<f32>,
};

@group(0) @binding(0) var main_texture: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var<uniform> draw: DrawUniform;
@group(2) @binding(0) var destination_texture: texture_2d<f32>;
@group(2) @binding(1) var destination_sampler: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_point: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_point: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let pixel = input.position.x * draw.view_a.xy
        + input.position.y * draw.view_b.xy + draw.view_origin.xy;
    output.position = vec4<f32>(
        pixel.x / draw.target_size.x * 2.0 - 1.0,
        1.0 - pixel.y / draw.target_size.y * 2.0,
        0.0,
        1.0,
    );
    output.uv = input.uv;
    output.mask_point = input.mask_point;
    return output;
}
"#;

pub(super) const EXTENDED_MASK: &str = r#"
@group(3) @binding(0) var mask_texture: texture_2d<f32>;
@group(3) @binding(1) var mask_sampler: sampler;

fn mask_alpha(point: vec2<f32>) -> f32 {
    let uv = (point - draw.mask_bounds.xy) / draw.mask_bounds.zw;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return select(0.0, 1.0, draw.mask_flags.y != 0u);
    }
    let sampled = textureSample(mask_texture, mask_sampler, uv).a;
    return select(sampled, 1.0 - sampled, draw.mask_flags.y != 0u);
}
"#;

pub(super) const EXTENDED_NO_MASK: &str = r#"
fn mask_alpha(_point: vec2<f32>) -> f32 {
    return 1.0;
}
"#;

pub(super) const EXTENDED_DRAW_BODY: &str = r#"
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var source = textureSample(main_texture, main_sampler, input.uv);
    source = vec4<f32>(source.rgb * draw.multiply_color.rgb, source.a);
    source = vec4<f32>(source.rgb + draw.screen_color.rgb - source.rgb * draw.screen_color.rgb, source.a * draw.opacity);
    if (draw.mask_flags.x != 0u) {
        source *= mask_alpha(input.mask_point);
    }
    let destination_uv = input.position.xy / draw.target_size;
    let destination = to_straight(textureSample(destination_texture, destination_sampler, destination_uv));
    return composite_blend(to_straight(source), destination, draw.blend_modes.x, draw.blend_modes.y);
}
"#;

pub(super) const EXTENDED_COMPOSITE_BODY: &str = r#"
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var source = textureSample(main_texture, main_sampler, input.uv);
    source = vec4<f32>(source.rgb * draw.multiply_color.rgb, source.a);
    source = vec4<f32>(source.rgb + draw.screen_color.rgb * source.a - source.rgb * draw.screen_color.rgb, source.a);
    source *= draw.opacity;
    if (draw.mask_flags.x != 0u) {
        source *= mask_alpha(input.mask_point);
    }
    let destination_uv = input.position.xy / draw.target_size;
    let destination = to_straight(textureSample(destination_texture, destination_sampler, destination_uv));
    return composite_blend(to_straight(source), destination, draw.blend_modes.x, draw.blend_modes.y);
}
"#;

pub(super) fn extended_shader_source(composite: bool, masked: bool) -> String {
    let body = if composite {
        EXTENDED_COMPOSITE_BODY
    } else {
        EXTENDED_DRAW_BODY
    };
    [
        EXTENDED_HEADER,
        include_str!("../shaders/extended_blend.wgsl"),
        if masked {
            EXTENDED_MASK
        } else {
            EXTENDED_NO_MASK
        },
        body,
    ]
    .concat()
}
