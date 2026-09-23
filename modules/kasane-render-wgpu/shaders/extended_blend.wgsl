fn to_straight(value: vec4<f32>) -> vec4<f32> {
    if (abs(value.a) < 0.00001) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(value.rgb / value.a, value.a);
}

fn color_burn(source: f32, destination: f32) -> f32 {
    if (abs(destination - 1.0) < 0.000001) {
        return 1.0;
    }
    if (abs(source) < 0.000001) {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - destination) / source);
}

fn color_dodge(source: f32, destination: f32) -> f32 {
    if (destination <= 0.0) {
        return 0.0;
    }
    if (source >= 1.0) {
        return 1.0;
    }
    return min(1.0, destination / (1.0 - source));
}

fn soft_light(source: f32, destination: f32) -> f32 {
    let low = destination - (1.0 - 2.0 * source) * destination * (1.0 - destination);
    let mid = destination + (2.0 * source - 1.0) * destination
        * ((16.0 * destination - 12.0) * destination + 3.0);
    let high = destination + (2.0 * source - 1.0) * (sqrt(destination) - destination);
    return select(select(high, mid, destination <= 0.25), low, source <= 0.5);
}

fn luma(value: vec3<f32>) -> f32 {
    return dot(value, vec3<f32>(0.30, 0.59, 0.11));
}

fn saturation(value: vec3<f32>) -> f32 {
    return max(value.r, max(value.g, value.b)) - min(value.r, min(value.g, value.b));
}

fn clip_color(value: vec3<f32>) -> vec3<f32> {
    let lum = luma(value);
    let hi = max(value.r, max(value.g, value.b));
    let lo = min(value.r, min(value.g, value.b));
    var result = value;
    if (lo < 0.0 && lum != lo) {
        result = vec3<f32>(lum) + (result - vec3<f32>(lum)) * lum / (lum - lo);
    }
    if (hi > 1.0 && hi != lum) {
        result = vec3<f32>(lum) + (result - vec3<f32>(lum)) * (1.0 - lum) / (hi - lum);
    }
    return result;
}

fn set_luma(value: vec3<f32>, lum: f32) -> vec3<f32> {
    return clip_color(value + vec3<f32>(lum - luma(value)));
}

fn set_saturation(value: vec3<f32>, sat: f32) -> vec3<f32> {
    let hi = max(value.r, max(value.g, value.b));
    let lo = min(value.r, min(value.g, value.b));
    let mid = value.r + value.g + value.b - hi - lo;
    let out_hi = select(0.0, sat, lo < hi);
    let out_mid = select(0.0, (mid - lo) * sat / (hi - lo), lo < hi);
    if (value.r == hi) {
        return select(vec3<f32>(out_hi, 0.0, out_mid), vec3<f32>(out_hi, out_mid, 0.0), value.b < value.g);
    }
    if (value.g == hi) {
        return select(vec3<f32>(out_mid, out_hi, 0.0), vec3<f32>(0.0, out_hi, out_mid), value.r < value.b);
    }
    return select(vec3<f32>(0.0, out_mid, out_hi), vec3<f32>(out_mid, 0.0, out_hi), value.g < value.r);
}

fn color_blend(source: vec3<f32>, destination: vec3<f32>, mode: u32) -> vec3<f32> {
    if (mode == 0u) { return source; }
    if (mode == 1u || mode == 3u) { return min(source + destination, vec3<f32>(1.0)); }
    if (mode == 2u || mode == 6u) { return source * destination; }
    if (mode == 4u) { return source + destination; }
    if (mode == 5u) { return min(source, destination); }
    if (mode == 7u) { return vec3<f32>(color_burn(source.r, destination.r), color_burn(source.g, destination.g), color_burn(source.b, destination.b)); }
    if (mode == 8u) { return max(vec3<f32>(0.0), source + destination - vec3<f32>(1.0)); }
    if (mode == 9u) { return max(source, destination); }
    if (mode == 10u) { return source + destination - source * destination; }
    if (mode == 11u) { return vec3<f32>(color_dodge(source.r, destination.r), color_dodge(source.g, destination.g), color_dodge(source.b, destination.b)); }
    if (mode == 12u) { return mix(2.0 * source * destination, vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - source) * (vec3<f32>(1.0) - destination), step(vec3<f32>(0.5), destination)); }
    if (mode == 13u) { return vec3<f32>(soft_light(source.r, destination.r), soft_light(source.g, destination.g), soft_light(source.b, destination.b)); }
    if (mode == 14u) { return mix(2.0 * source * destination, vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - source) * (vec3<f32>(1.0) - destination), step(vec3<f32>(0.5), source)); }
    if (mode == 15u) { return mix(max(vec3<f32>(0.0), 2.0 * source + destination - vec3<f32>(1.0)), min(vec3<f32>(1.0), 2.0 * (source - vec3<f32>(0.5)) + destination), step(vec3<f32>(0.5), source)); }
    if (mode == 16u) { return set_luma(set_saturation(source, saturation(destination)), luma(destination)); }
    return set_luma(source, luma(destination));
}

fn alpha_blend(color: vec3<f32>, source: vec4<f32>, destination: vec4<f32>, mode: u32) -> vec4<f32> {
    var weights: vec3<f32>;
    if (mode == 1u) {
        weights = vec3<f32>(source.a * destination.a, 0.0, destination.a * (1.0 - source.a));
    } else if (mode == 2u) {
        weights = vec3<f32>(0.0, 0.0, destination.a * (1.0 - source.a));
    } else if (mode == 3u) {
        weights = vec3<f32>(min(source.a, destination.a), max(source.a - destination.a, 0.0), max(destination.a - source.a, 0.0));
    } else if (mode == 4u) {
        weights = vec3<f32>(max(source.a + destination.a - 1.0, 0.0), min(source.a, 1.0 - destination.a), min(destination.a, 1.0 - source.a));
    } else {
        weights = vec3<f32>(source.a * destination.a, source.a * (1.0 - destination.a), destination.a * (1.0 - source.a));
    }
    return vec4<f32>(color * weights.x + source.rgb * weights.y + destination.rgb * weights.z, weights.x + weights.y + weights.z);
}

fn composite_blend(source: vec4<f32>, destination: vec4<f32>, color_mode: u32, alpha_mode: u32) -> vec4<f32> {
    if (color_mode == 1u) {
        return vec4<f32>(source.rgb * source.a + destination.rgb * destination.a, destination.a);
    }
    if (color_mode == 2u) {
        return vec4<f32>((source.rgb * source.a + vec3<f32>(1.0 - source.a)) * destination.rgb * destination.a, destination.a);
    }
    return alpha_blend(color_blend(source.rgb, destination.rgb, color_mode), source, destination, alpha_mode);
}
