vec4 to_straight(vec4 value) {
    if (abs(value.a) < 0.00001) {
        value.rgb = vec3(0.0);
    } else {
        value.rgb /= value.a;
    }
    return value;
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
    float mid = destination + (2.0 * source - 1.0) * destination * ((16.0 * destination - 12.0) * destination + 3.0);
    float high = destination + (2.0 * source - 1.0) * (sqrt(destination) - destination);
    return source <= 0.5 ? low : (destination <= 0.25 ? mid : high);
}

float luma(vec3 value) { return dot(value, vec3(0.30, 0.59, 0.11)); }
float saturation(vec3 value) { return max(value.r, max(value.g, value.b)) - min(value.r, min(value.g, value.b)); }

vec3 clip_color(vec3 value) {
    float lum = luma(value);
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    if (lo < 0.0) value = lum + (value - vec3(lum)) * lum / (lum - lo);
    if (hi > 1.0) value = lum + (value - vec3(lum)) * (1.0 - lum) / (hi - lum);
    return value;
}

vec3 set_luma(vec3 value, float lum) { return clip_color(value + vec3(lum - luma(value))); }

vec3 set_saturation(vec3 value, float sat) {
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    float mid = value.r + value.g + value.b - hi - lo;
    float out_hi = lo < hi ? sat : 0.0;
    float out_mid = lo < hi ? (mid - lo) * sat / (hi - lo) : 0.0;
    if (value.r == hi) return value.b < value.g ? vec3(out_hi, out_mid, 0.0) : vec3(out_hi, 0.0, out_mid);
    if (value.g == hi) return value.r < value.b ? vec3(0.0, out_hi, out_mid) : vec3(out_mid, out_hi, 0.0);
    return value.g < value.r ? vec3(out_mid, 0.0, out_hi) : vec3(0.0, out_mid, out_hi);
}

vec3 color_blend(vec3 source, vec3 destination, int mode) {
    if (mode == 0) return source;
    if (mode == 1 || mode == 3) return min(source + destination, vec3(1.0));
    if (mode == 2) return source * destination;
    if (mode == 4) return source + destination;
    if (mode == 5) return min(source, destination);
    if (mode == 6) return source * destination;
    if (mode == 7) return vec3(color_burn(source.r, destination.r), color_burn(source.g, destination.g), color_burn(source.b, destination.b));
    if (mode == 8) return max(vec3(0.0), source + destination - vec3(1.0));
    if (mode == 9) return max(source, destination);
    if (mode == 10) return source + destination - source * destination;
    if (mode == 11) return vec3(color_dodge(source.r, destination.r), color_dodge(source.g, destination.g), color_dodge(source.b, destination.b));
    if (mode == 12) return mix(2.0 * source * destination, vec3(1.0) - 2.0 * (vec3(1.0) - source) * (vec3(1.0) - destination), step(vec3(0.5), destination));
    if (mode == 13) return vec3(soft_light(source.r, destination.r), soft_light(source.g, destination.g), soft_light(source.b, destination.b));
    if (mode == 14) return mix(2.0 * source * destination, vec3(1.0) - 2.0 * (vec3(1.0) - source) * (vec3(1.0) - destination), step(vec3(0.5), source));
    if (mode == 15) return mix(max(vec3(0.0), 2.0 * source + destination - vec3(1.0)), min(vec3(1.0), 2.0 * (source - vec3(0.5)) + destination), step(vec3(0.5), source));
    if (mode == 16) return set_luma(set_saturation(source, saturation(destination)), luma(destination));
    return set_luma(source, luma(destination));
}

vec4 alpha_blend(vec3 color, vec4 source, vec4 destination, int mode) {
    vec3 weights;
    if (mode == 1) {
        weights = vec3(source.a * destination.a, 0.0, destination.a * (1.0 - source.a));
    } else if (mode == 2) {
        weights = vec3(0.0, 0.0, destination.a * (1.0 - source.a));
    } else if (mode == 3) {
        weights = vec3(min(source.a, destination.a), max(source.a - destination.a, 0.0), max(destination.a - source.a, 0.0));
    } else if (mode == 4) {
        weights = vec3(max(source.a + destination.a - 1.0, 0.0), min(source.a, 1.0 - destination.a), min(destination.a, 1.0 - source.a));
    } else {
        weights = vec3(source.a * destination.a, source.a * (1.0 - destination.a), destination.a * (1.0 - source.a));
    }
    return vec4(color * weights.x + source.rgb * weights.y + destination.rgb * weights.z, weights.x + weights.y + weights.z);
}

// SDK compatibility modes ignore the Alpha selector and preserve destination
// alpha. They are fixed-function additive/multiply, not the newer OVER modes.
vec4 composite_blend(vec4 source, vec4 destination, int color_mode, int alpha_mode) {
    if (color_mode == 1) {
        return vec4(source.rgb * source.a + destination.rgb * destination.a, destination.a);
    }
    if (color_mode == 2) {
        return vec4((source.rgb * source.a + vec3(1.0 - source.a)) * destination.rgb * destination.a, destination.a);
    }
    return alpha_blend(color_blend(source.rgb, destination.rgb, color_mode), source, destination, alpha_mode);
}
