use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

pub fn rect_vertices(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> [Vertex; 4] {
    [
        Vertex { position: [x, y], color },
        Vertex { position: [x + w, y], color },
        Vertex { position: [x, y + h], color },
        Vertex { position: [x + w, y + h], color },
    ]
}

pub const RECT_INDICES: [u16; 6] = [0, 1, 2, 1, 3, 2];
