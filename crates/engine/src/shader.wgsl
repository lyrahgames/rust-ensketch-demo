struct Camera { view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}
struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
}
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.normal = input.normal;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(input.normal);
    let lighting = 0.25 + 0.75 * max(dot(n, normalize(vec3<f32>(0.4, 0.7, 0.6))), 0.0);
    return vec4<f32>(vec3<f32>(0.35, 0.67, 0.92) * lighting, 1.0);
}
