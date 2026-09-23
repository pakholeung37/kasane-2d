shader_type canvas_item;
render_mode blend_mix, unshaded;
uniform sampler2D main_texture : filter_linear_mipmap, repeat_disable;

void fragment() {
    COLOR = vec4(0.0, 0.0, 0.0, texture(main_texture, UV).a);
}
