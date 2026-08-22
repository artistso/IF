struct VertexInput {
    @location(0) center: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) edge: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) edge: f32,
};

@vertex
fn vs_main(input: VertexInput, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    let local = corners[vertex_index];

    var output: VertexOutput;
    output.position = vec4<f32>(input.center + local * input.half_size, 0.0, 1.0);
    output.local = local;
    output.color = input.color;
    output.edge = input.edge;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let distance_from_center = length(input.local);
    if distance_from_center > 1.0 {
        discard;
    }
    let edge = clamp(input.edge, 0.001, 1.0);
    let coverage = 1.0 - smoothstep(1.0 - edge, 1.0, distance_from_center);
    return vec4<f32>(input.color.rgb, input.color.a * coverage);
}
