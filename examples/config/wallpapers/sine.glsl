// Green sine waves drifting over a dark green gradient (Shadertoy-style, see the setting `wallpaper`)
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec3 color = mix(vec3(0.01, 0.04, 0.02), vec3(0.02, 0.10, 0.05), uv.y);
    for (int i = 0; i < 3; i++) {
        float layer = float(i);
        float wave = 0.5
            + 0.12 * sin(uv.x * (6.0 + 2.0 * layer) + iTime * (0.6 + 0.25 * layer) + layer * 2.1)
            + 0.04 * sin(uv.x * 17.0 - iTime * 0.9 + layer);
        float glow = 0.0035 / max(abs(uv.y - wave), 0.0005);
        color += vec3(0.25, 0.95, 0.45) * glow * (0.55 - 0.15 * layer);
    }
    fragColor = vec4(color, 1.0);
}
