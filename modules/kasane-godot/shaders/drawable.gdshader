shader_type canvas_item;
render_mode __BLEND__, unshaded;

uniform sampler2D main_texture : filter_linear_mipmap, repeat_disable;
uniform vec3 multiply_color;
uniform vec3 screen_color;
uniform float opacity;
uniform bool inverted;
uniform bool masked;
uniform sampler2D mask_texture : filter_linear, repeat_disable;
uniform vec4 mask_bounds;
varying vec2 point;

void vertex() {
    point = VERTEX;
}

void fragment() {
    vec4 c = texture(main_texture, UV);
    c.rgb *= multiply_color;
    c.rgb = c.rgb + screen_color - c.rgb * screen_color;
    float a = c.a * opacity;
    if (masked) {
        vec2 uv = (point - mask_bounds.xy) / mask_bounds.zw;
        float mask = 0.0;
        if (all(greaterThanEqual(uv, vec2(0.0))) && all(lessThanEqual(uv, vec2(1.0)))) {
            mask = texture(mask_texture, uv).a;
        }
        a *= inverted ? 1.0 - mask : mask;
    }
    __OUTPUT__
}
