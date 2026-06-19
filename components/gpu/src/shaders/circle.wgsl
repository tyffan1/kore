struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) center: vec2<f32>,
    @location(3) radius: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) circle_center: vec2<f32>,
    @location(3) circle_radius: f32,
};

struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: Viewport;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = (in.position.x / viewport.size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.position.y / viewport.size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    out.world_pos = in.position;
    out.circle_center = in.center;
    out.circle_radius = in.radius;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = distance(in.world_pos, in.circle_center);
    if dist > in.circle_radius {
        discard;
    }
    let edge = smoothstep(0.45, 0.5, dist / in.circle_radius);
    let final_color = mix(in.color, vec4<f32>(0.0, 0.0, 0.0, 0.0), edge * 0.2);
    return final_color;
}
