shader_type canvas_item;
render_mode blend_disabled, unshaded;

uniform sampler2D main_texture : filter_linear_mipmap, repeat_disable;
uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_linear;
uniform int color_blend_mode;
uniform int alpha_blend_mode;
uniform vec3 multiply_color;
uniform vec3 screen_color;
uniform float opacity;
uniform bool inverted;
uniform bool masked;
uniform sampler2D mask_texture : filter_linear, repeat_disable;
uniform vec4 mask_bounds;
varying vec2 point;

__BLEND_FUNCTIONS__

void vertex() {
    point = VERTEX;
}

void fragment() {
    vec4 source = texture(main_texture, UV);
    source.rgb *= multiply_color;
    source.rgb = source.rgb + screen_color - source.rgb * screen_color;
    source.a *= opacity;
    if (masked) {
        vec2 mask_uv = (point - mask_bounds.xy) / mask_bounds.zw;
        float mask = 0.0;
        if (all(greaterThanEqual(mask_uv, vec2(0.0))) && all(lessThanEqual(mask_uv, vec2(1.0)))) {
            mask = texture(mask_texture, mask_uv).a;
        }
        source.a *= inverted ? 1.0 - mask : mask;
    }
    vec4 destination = to_straight(textureLod(screen_texture, SCREEN_UV, 0.0));
    COLOR = composite_blend(source, destination, color_blend_mode, alpha_blend_mode);
}
