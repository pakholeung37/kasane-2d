// SPDX-License-Identifier: MIT
// The blend functions are compiled directly from the pinned official SDK.
GLuint compile_shader(GLenum type, const std::string& code) {
    GLuint shader = glCreateShader(type); const char* text = code.c_str();
    glShaderSource(shader, 1, &text, nullptr); glCompileShader(shader);
    GLint ok; glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok) { char log[8192]; glGetShaderInfoLog(shader, sizeof(log), nullptr, log); throw std::runtime_error(log); }
    return shader;
}
std::string shader_text(const char* name) { auto bytes = read_file(shaders/name); return {bytes.begin(),bytes.end()}; }
void blend_matrix(const char* input, const char* output) {
    std::ifstream data(input); int count; data >> count;
    struct Sample { float source[4], destination[4], opacity, mask; int premultiplied; };
    std::vector<Sample> samples(count);
    for (auto& s : samples) { for (float& v:s.source) data>>v; for(float& v:s.destination)data>>v; data>>s.opacity>>s.mask>>s.premultiplied; }
    if (!data || count < 1) throw std::runtime_error("Invalid blend matrix input");
    const int width = count * 8, height = 90 * 8;
    GLuint texture, fbo; glGenTextures(1,&texture); glBindTexture(GL_TEXTURE_2D,texture);
    glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA8,width,height,0,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);
    glGenFramebuffers(1,&fbo); glBindFramebuffer(GL_FRAMEBUFFER,fbo); glFramebufferTexture2D(GL_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,texture,0);
    glDisable(GL_BLEND); glDisable(GL_DEPTH_TEST); glDisable(GL_CULL_FACE);
    for (int color=0;color<18;++color) for(int alpha=0;alpha<5;++alpha) {
        std::string code = "#version 120\n#define CSM_COLOR_BLEND_MODE " + std::to_string(color < 3 ? 0 : color-2) + "\n#define CSM_ALPHA_BLEND_MODE " + std::to_string(alpha) + "\n";
        code += shader_text("FragShaderSrcColorBlend.frag") + shader_text("FragShaderSrcAlphaBlend.frag");
        code += "\nuniform vec4 source; uniform vec4 destination; void main() {";
        if(color==1) code += "gl_FragColor=vec4(source.rgb*source.a+destination.rgb*destination.a,destination.a);";
        else if(color==2) code += "gl_FragColor=vec4((source.rgb*source.a+vec3(1.0-source.a))*destination.rgb*destination.a,destination.a);";
        else code += "gl_FragColor=AlphaBlend(ColorBlend(source.rgb,destination.rgb),source,destination);";
        code += "}";
        auto vertex=compile_shader(GL_VERTEX_SHADER,"#version 120\nvoid main(){gl_Position=gl_Vertex;}");
        auto fragment=compile_shader(GL_FRAGMENT_SHADER,code); auto program=glCreateProgram();
        glAttachShader(program,vertex);glAttachShader(program,fragment);glLinkProgram(program);
        GLint linked;glGetProgramiv(program,GL_LINK_STATUS,&linked);if(!linked)throw std::runtime_error("Matrix program link failed");
        glUseProgram(program);
        for(int i=0;i<count;++i) {
            auto s=samples[i];
            if(s.premultiplied) { if(s.source[3]<0.00001f)std::fill_n(s.source,3,0.f);else for(int c=0;c<3;++c)s.source[c]/=s.source[3]; }
            s.source[3]*=s.opacity*s.mask;
            if(s.destination[3]<0.00001f) std::fill_n(s.destination,3,0.f);
            else for(int c=0;c<3;++c)s.destination[c]/=s.destination[3];
            glUniform4fv(glGetUniformLocation(program,"source"),1,s.source);glUniform4fv(glGetUniformLocation(program,"destination"),1,s.destination);
            glViewport(i*8,height-(color*5+alpha+1)*8,8,8);
            glBegin(GL_TRIANGLE_STRIP);glVertex2f(-1,-1);glVertex2f(1,-1);glVertex2f(-1,1);glVertex2f(1,1);glEnd();
        }
        glDeleteProgram(program);glDeleteShader(vertex);glDeleteShader(fragment);
    }
    save_image(output,width,height);
    std::cout << "{\"color_modes\":18,\"alpha_modes\":5,\"samples_per_mode\":" << count << "}\n";
}
