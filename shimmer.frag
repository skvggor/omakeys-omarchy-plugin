#version 440

// Sweep-highlight pass for a layered item (the key text). The item's own
// rendering arrives as `source`; the highlight is gated on the source alpha
// so brightness is only ever added where a glyph exists.

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4  qt_Matrix;      // required by Qt Quick
    float qt_Opacity;     // required by Qt Quick
    float sweep;          // 0..1, highlight centre along the item width
    float bandWidth;      // highlight half-width, normalized 0..1
    float strength;       // highlight intensity, 0..1
};

layout(binding = 1) uniform sampler2D source;

void main() {
    vec4 c = texture(source, qt_TexCoord0);
    float dist = abs(qt_TexCoord0.x - sweep);
    float band = 1.0 - smoothstep(0.0, bandWidth, dist);
    vec3 lit = c.rgb + vec3(strength) * band * c.a;
    fragColor = vec4(lit, c.a) * qt_Opacity;
}