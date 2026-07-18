// Shared gradient-palette sampling. Concatenated into both standard.wgsl and
// perturbation.wgsl at shader-module creation time (WGSL has no #include).
// Both shaders reserve bindings 1/2 in their bind group layout for these.

@group(0) @binding(1) var<storage, read> stop_positions: array<f32>;
@group(0) @binding(2) var<storage, read> stop_colors: array<vec4<f32>>;

fn sample_palette(t_in: f32, stop_count: u32) -> vec4<f32> {
    let t = fract(t_in);
    if (stop_count == 0u) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    if (stop_count == 1u) {
        return stop_colors[0];
    }
    if (t <= stop_positions[0]) {
        return stop_colors[0];
    }
    if (t >= stop_positions[stop_count - 1u]) {
        return stop_colors[stop_count - 1u];
    }
    for (var i: u32 = 0u; i < stop_count - 1u; i = i + 1u) {
        let a_pos = stop_positions[i];
        let b_pos = stop_positions[i + 1u];
        if (t >= a_pos && t <= b_pos) {
            let span = max(b_pos - a_pos, 0.00001);
            let f = (t - a_pos) / span;
            return mix(stop_colors[i], stop_colors[i + 1u], f);
        }
    }
    return stop_colors[stop_count - 1u];
}
