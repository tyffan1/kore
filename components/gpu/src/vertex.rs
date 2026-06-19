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

/// A vertex with texture coordinates, used for text glyph rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct TextVertex {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: [f32; 4],
}

impl TextVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
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
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CircleVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub center: [f32; 2],
    pub radius: f32,
}

impl CircleVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<CircleVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 32, shader_location: 3, format: wgpu::VertexFormat::Float32 },
        ],
    };
}

pub fn circle_quad_vertices(cx: f32, cy: f32, radius: f32, color: [f32; 4]) -> [CircleVertex; 4] {
    let x = cx - radius;
    let y = cy - radius;
    let s = 2.0 * radius;
    [
        CircleVertex { position: [x, y], color, center: [cx, cy], radius },
        CircleVertex { position: [x + s, y], color, center: [cx, cy], radius },
        CircleVertex { position: [x, y + s], color, center: [cx, cy], radius },
        CircleVertex { position: [x + s, y + s], color, center: [cx, cy], radius },
    ]
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

/// Generate four vertices for a textured quad.
pub fn text_quad_vertices(
    dest_x: f32,
    dest_y: f32,
    dest_w: f32,
    dest_h: f32,
    uv_x: f32,
    uv_y: f32,
    uv_w: f32,
    uv_h: f32,
    color: [f32; 4],
) -> [TextVertex; 4] {
    let u2 = uv_x + uv_w;
    let v2 = uv_y + uv_h;
    [
        TextVertex {
            position: [dest_x, dest_y],
            tex_coord: [uv_x, uv_y],
            color,
        },
        TextVertex {
            position: [dest_x + dest_w, dest_y],
            tex_coord: [u2, uv_y],
            color,
        },
        TextVertex {
            position: [dest_x, dest_y + dest_h],
            tex_coord: [uv_x, v2],
            color,
        },
        TextVertex {
            position: [dest_x + dest_w, dest_y + dest_h],
            tex_coord: [u2, v2],
            color,
        },
    ]
}
