#include <metal_stdlib>
using namespace metal;

struct Vertex {
    float2 position;
    float2 uv;
    float2 mask_point;
};

struct Uniform {
    float4 target_size;
    float4 multiply_color;
    float4 screen_color;
    float4 mask_bounds;
    float4 opacity;
    uint4 flags;
    uint4 modes;
    float4 view_a;
    float4 view_b;
    float4 view_origin;
    float4 mask_a;
    float4 mask_b;
    float4 mask_origin;
    float4 mask_region;
};

struct Varying {
    float4 position [[position]];
    float2 uv;
    float2 mask_point;
};

vertex Varying vs_main(uint id [[vertex_id]],
                       const device Vertex* vertices [[buffer(0)]],
                       constant Uniform& u [[buffer(1)]]) {
    Vertex v = vertices[id];
    float2 pixel = v.position.x * u.view_a.xy
                 + v.position.y * u.view_b.xy + u.view_origin.xy;
    Varying out;
    out.position = float4(pixel.x / u.target_size.x * 2.0 - 1.0,
                          1.0 - pixel.y / u.target_size.y * 2.0, 0.0, 1.0);
    out.uv = v.uv;
    out.mask_point = v.mask_point.x * u.mask_a.xy
                   + v.mask_point.y * u.mask_b.xy + u.mask_origin.xy;
    return out;
}

float mask_alpha(Varying in, constant Uniform& u,
                 texture2d<float> mask, sampler mask_sampler) {
    if (u.flags.x == 0) return 1.0;
    float2 uv = (in.mask_point - u.mask_bounds.xy) / u.mask_bounds.zw;
    if (any(uv < 0.0) || any(uv > 1.0)) return u.flags.y != 0 ? 1.0 : 0.0;
    // Clamp within the original mask's texel centers before atlas mapping,
    // preserving standalone ClampToEdge sampling without adjacent-tile bleed.
    float2 texture_size = float2(mask.get_width(), mask.get_height());
    float2 half_texel = 0.5 / (texture_size * u.mask_region.zw);
    uv = clamp(uv, half_texel, 1.0 - half_texel);
    float alpha = mask.sample(mask_sampler, u.mask_region.xy + uv * u.mask_region.zw).a;
    return u.flags.y != 0 ? 1.0 - alpha : alpha;
}

fragment float4 fs_draw(Varying in [[stage_in]],
                        constant Uniform& u [[buffer(1)]],
                        texture2d<float> source [[texture(0)]],
                        texture2d<float> mask [[texture(1)]],
                        sampler source_sampler [[sampler(0)]],
                        sampler mask_sampler [[sampler(1)]]) {
    float4 sampled = source.sample(source_sampler, in.uv);
    float3 multiplied = sampled.rgb * u.multiply_color.rgb;
    float3 rgb = multiplied + u.screen_color.rgb * sampled.a - multiplied * u.screen_color.rgb;
    float factor = u.opacity.x * mask_alpha(in, u, mask, mask_sampler);
    float alpha = sampled.a * factor;
    if (u.flags.z == 1) return float4(rgb * factor, 0.0);
    if (u.flags.z == 2) return float4(rgb * factor + float3(1.0 - alpha), 1.0);
    return float4(rgb * factor, alpha);
}

fragment float4 fs_composite(Varying in [[stage_in]],
                             constant Uniform& u [[buffer(1)]],
                             texture2d<float> source [[texture(0)]],
                             texture2d<float> mask [[texture(1)]],
                             sampler source_sampler [[sampler(0)]],
                             sampler mask_sampler [[sampler(1)]]) {
    float4 sampled = source.sample(source_sampler, in.uv);
    float3 rgb = sampled.rgb * u.multiply_color.rgb
               + u.screen_color.rgb * sampled.a
               - sampled.rgb * u.screen_color.rgb;
    float factor = u.opacity.x * mask_alpha(in, u, mask, mask_sampler);
    return float4(rgb * factor, sampled.a * factor);
}

fragment float4 fs_mask(Varying in [[stage_in]],
                        texture2d<float> source [[texture(0)]],
                        sampler source_sampler [[sampler(0)]]) {
    return float4(0.0, 0.0, 0.0, source.sample(source_sampler, in.uv).a);
}

fragment float4 fs_present(Varying in [[stage_in]],
                           texture2d<float> source [[texture(0)]],
                           sampler source_sampler [[sampler(0)]]) {
    return source.sample(source_sampler, in.uv);
}

float4 to_straight(float4 value) {
    return abs(value.a) < 0.00001 ? float4(0.0) : float4(value.rgb / value.a, value.a);
}

float color_burn(float source, float destination) {
    if (abs(destination - 1.0) < 0.000001) return 1.0;
    if (abs(source) < 0.000001) return 0.0;
    return 1.0 - min(1.0, (1.0 - destination) / source);
}

float color_dodge(float source, float destination) {
    if (destination <= 0.0) return 0.0;
    if (source >= 1.0) return 1.0;
    return min(1.0, destination / (1.0 - source));
}

float soft_light(float source, float destination) {
    float low = destination - (1.0 - 2.0 * source) * destination * (1.0 - destination);
    float mid = destination + (2.0 * source - 1.0) * destination
              * ((16.0 * destination - 12.0) * destination + 3.0);
    float high = destination + (2.0 * source - 1.0) * (sqrt(destination) - destination);
    return source <= 0.5 ? low : (destination <= 0.25 ? mid : high);
}

float luma(float3 value) { return dot(value, float3(0.30, 0.59, 0.11)); }
float saturation(float3 value) { return max(value.r, max(value.g, value.b)) - min(value.r, min(value.g, value.b)); }

float3 clip_color(float3 value) {
    float lum = luma(value);
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    if (lo < 0.0 && lum != lo) value = float3(lum) + (value - float3(lum)) * lum / (lum - lo);
    if (hi > 1.0 && hi != lum) value = float3(lum) + (value - float3(lum)) * (1.0 - lum) / (hi - lum);
    return value;
}

float3 set_luma(float3 value, float lum) { return clip_color(value + float3(lum - luma(value))); }

float3 set_saturation(float3 value, float sat) {
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    float mid = value.r + value.g + value.b - hi - lo;
    float out_hi = lo < hi ? sat : 0.0;
    float out_mid = lo < hi ? (mid - lo) * sat / (hi - lo) : 0.0;
    if (value.r == hi) return value.b < value.g ? float3(out_hi, out_mid, 0.0) : float3(out_hi, 0.0, out_mid);
    if (value.g == hi) return value.r < value.b ? float3(0.0, out_hi, out_mid) : float3(out_mid, out_hi, 0.0);
    return value.g < value.r ? float3(out_mid, 0.0, out_hi) : float3(0.0, out_mid, out_hi);
}

float3 color_blend(float3 source, float3 destination, uint mode) {
    if (mode == 0) return source;
    if (mode == 1 || mode == 3) return min(source + destination, float3(1.0));
    if (mode == 2 || mode == 6) return source * destination;
    if (mode == 4) return source + destination;
    if (mode == 5) return min(source, destination);
    if (mode == 7) return float3(color_burn(source.r, destination.r), color_burn(source.g, destination.g), color_burn(source.b, destination.b));
    if (mode == 8) return max(float3(0.0), source + destination - float3(1.0));
    if (mode == 9) return max(source, destination);
    if (mode == 10) return source + destination - source * destination;
    if (mode == 11) return float3(color_dodge(source.r, destination.r), color_dodge(source.g, destination.g), color_dodge(source.b, destination.b));
    if (mode == 12) return mix(2.0 * source * destination, float3(1.0) - 2.0 * (float3(1.0) - source) * (float3(1.0) - destination), step(float3(0.5), destination));
    if (mode == 13) return float3(soft_light(source.r, destination.r), soft_light(source.g, destination.g), soft_light(source.b, destination.b));
    if (mode == 14) return mix(2.0 * source * destination, float3(1.0) - 2.0 * (float3(1.0) - source) * (float3(1.0) - destination), step(float3(0.5), source));
    if (mode == 15) return mix(max(float3(0.0), 2.0 * source + destination - float3(1.0)), min(float3(1.0), 2.0 * (source - float3(0.5)) + destination), step(float3(0.5), source));
    if (mode == 16) return set_luma(set_saturation(source, saturation(destination)), luma(destination));
    return set_luma(source, luma(destination));
}

float4 alpha_blend(float3 color, float4 source, float4 destination, uint mode) {
    float3 weights;
    if (mode == 1) weights = float3(source.a * destination.a, 0.0, destination.a * (1.0 - source.a));
    else if (mode == 2) weights = float3(0.0, 0.0, destination.a * (1.0 - source.a));
    else if (mode == 3) weights = float3(min(source.a, destination.a), max(source.a - destination.a, 0.0), max(destination.a - source.a, 0.0));
    else if (mode == 4) weights = float3(max(source.a + destination.a - 1.0, 0.0), min(source.a, 1.0 - destination.a), min(destination.a, 1.0 - source.a));
    else weights = float3(source.a * destination.a, source.a * (1.0 - destination.a), destination.a * (1.0 - source.a));
    return float4(color * weights.x + source.rgb * weights.y + destination.rgb * weights.z,
                  weights.x + weights.y + weights.z);
}

float4 composite_blend(float4 source, float4 destination, uint color_mode, uint alpha_mode) {
    if (color_mode == 1) return float4(source.rgb * source.a + destination.rgb * destination.a, destination.a);
    if (color_mode == 2) return float4((source.rgb * source.a + float3(1.0 - source.a)) * destination.rgb * destination.a, destination.a);
    return alpha_blend(color_blend(source.rgb, destination.rgb, color_mode), source, destination, alpha_mode);
}

fragment float4 fs_extended_draw(Varying in [[stage_in]],
                                 constant Uniform& u [[buffer(1)]],
                                 texture2d<float> source_tex [[texture(0)]],
                                 texture2d<float> mask [[texture(1)]],
                                 texture2d<float> destination_tex [[texture(2)]],
                                 sampler source_sampler [[sampler(0)]],
                                 sampler mask_sampler [[sampler(1)]],
                                 sampler destination_sampler [[sampler(2)]]) {
    float4 source = to_straight(source_tex.sample(source_sampler, in.uv));
    source.rgb *= u.multiply_color.rgb;
    source.rgb = source.rgb + u.screen_color.rgb - source.rgb * u.screen_color.rgb;
    source.a *= u.opacity.x * mask_alpha(in, u, mask, mask_sampler);
    float2 destination_uv = in.position.xy / u.target_size.xy;
    float4 destination = to_straight(destination_tex.sample(destination_sampler, destination_uv));
    return composite_blend(source, destination, u.modes.x, u.modes.y);
}

fragment float4 fs_extended_composite(Varying in [[stage_in]],
                                      constant Uniform& u [[buffer(1)]],
                                      texture2d<float> source_tex [[texture(0)]],
                                      texture2d<float> mask [[texture(1)]],
                                      texture2d<float> destination_tex [[texture(2)]],
                                      sampler source_sampler [[sampler(0)]],
                                      sampler mask_sampler [[sampler(1)]],
                                      sampler destination_sampler [[sampler(2)]]) {
    float4 source = source_tex.sample(source_sampler, in.uv);
    source.rgb = source.rgb * u.multiply_color.rgb
               + u.screen_color.rgb * source.a - source.rgb * u.screen_color.rgb;
    source *= u.opacity.x * mask_alpha(in, u, mask, mask_sampler);
    float2 destination_uv = in.position.xy / u.target_size.xy;
    float4 destination = to_straight(destination_tex.sample(destination_sampler, destination_uv));
    return composite_blend(to_straight(source), destination, u.modes.x, u.modes.y);
}
